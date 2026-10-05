package dev.kyuyoung.hongsi.widget

import android.app.job.JobInfo
import android.app.job.JobParameters
import android.app.job.JobScheduler
import android.app.job.JobService
import android.content.ComponentName
import android.content.Context
import android.content.BroadcastReceiver
import android.content.Intent
import android.os.PersistableBundle
import android.os.Handler
import android.os.Looper
import org.json.JSONArray
import org.json.JSONObject
import java.util.concurrent.Executors
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.locks.ReentrantLock

internal class WidgetFailure(val status: Int, override val message: String) : Exception(message)
internal fun JSONObject.body(): Any {
    if (optInt("status") !in 200..299) {
        val error = optJSONObject("body")?.optJSONObject("error")
        val message = when (error?.text("code")) {
            "account_changed" -> "위젯을 새로고침해 주세요."
            "attendance_result_unknown" -> "출석 결과를 확인해 주세요."
            "timeout" -> "처리 결과를 확인해 주세요."
            "school_error", "school_unreachable" -> "학교 서버가 응답하지 않아요."
            else -> error?.text("message").orEmpty().ifBlank { "정보를 불러오지 못했어요." }
        }
        throw WidgetFailure(optInt("status"), message)
    }
    return get("body")
}

internal object WidgetSync {
    val worker = Executors.newFixedThreadPool(2)
    val main = Handler(Looper.getMainLooper())
    val locks = ConcurrentHashMap<String, ReentrantLock>()
    private const val PERIODIC = 7400
    fun installed(context: Context) = WidgetKind.entries.filter { Widgets.ids(context, it).isNotEmpty() }
    fun schedule(context: Context) {
        val scheduler = context.getSystemService(JobScheduler::class.java)
        if (installed(context).isEmpty()) { (PERIODIC..PERIODIC + 7).forEach(scheduler::cancel); return }
        if (scheduler.getPendingJob(PERIODIC) == null) scheduler.schedule(JobInfo.Builder(PERIODIC, ComponentName(context, WidgetJobService::class.java))
            .setRequiredNetworkType(JobInfo.NETWORK_TYPE_ANY).setPeriodic(15 * 60_000L).setPersisted(true).build())
    }
    fun enqueue(context: Context, kind: WidgetKind) {
        schedule(context)
        val extras = PersistableBundle().apply { putString("kind", kind.name) }
        context.getSystemService(JobScheduler::class.java).schedule(JobInfo.Builder(PERIODIC + 1 + kind.ordinal, ComponentName(context, WidgetJobService::class.java))
            .setExtras(extras).setOverrideDeadline(0).build())
    }
    fun refresh(context: Context, kind: WidgetKind) {
        val resource = when { kind.attendance -> "lectures"; kind.deadlines -> "calendar"; kind == WidgetKind.SEAT -> "seats"; else -> "timetable" }
        val lock = locks.getOrPut(resource) { ReentrantLock() }
        if (!lock.tryLock()) return
        val before = WidgetData.read(context)
        val owner = before.text("owner")
        if (owner.isBlank()) { lock.unlock(); return }
        val patch = JSONObject()
        val updated = JSONObject()
        fun get(path: String) = WidgetNative.api(context, path, owner = owner).body()
        try {
            var preferences = before.optJSONObject("preferences") ?: JSONObject()
            var preferencesAt = before.optJSONObject("updatedAt")?.optLong("preferences") ?: 0
            if (resource in listOf("calendar", "timetable")) {
                runCatching { get("/api/preferences") as JSONObject }.onSuccess { saved ->
                    if (saved.optLong("updatedAt") > preferencesAt) {
                        preferences = saved
                        preferencesAt = saved.optLong("updatedAt")
                    }
                }.onFailure { if (it is WidgetFailure && it.status == 401) throw it }
                patch.put("preferences", JSONObject()
                    .put("timetableDisplay", if (preferences.text("timetableDisplay") == "full") "full" else "fit")
                    .put("semesterDisplay", if (preferences.text("semesterDisplay") == "all") "all" else "current"))
                updated.put("preferences", preferencesAt)
            }
            when (resource) {
                "lectures" -> {
                    val slots = (get("/api/timetable") as JSONObject).array("slots")
                    patch.put("slots", JSONArray(colorSlots(slots, before.array("slots"))))
                    val active = (get("/api/attendance/active") as JSONObject).array("items")
                    val pending = before.array("pendingReceipts").filter { it.text("date") == dateText() }
                    val unshared = pending.filter { receipt -> runCatching { WidgetNative.api(context, "/api/attendance/receipts", "PUT", JSONObject().put("receipt", receipt), owner).body(); false }.getOrDefault(true) }
                    patch.put("pendingReceipts", JSONArray(unshared))
                    val receipts = runCatching { (get("/api/attendance/receipts") as JSONArray).objects() }.getOrDefault(emptyList()) + pending
                    val courses = slots.filter { it.optInt("weekday") == weekday() }.map { it.text("code") }.filter(String::isNotBlank).distinct().mapNotNull { code ->
                        runCatching { get("/api/attendance/course?code=" + android.net.Uri.encode(code)) as JSONObject }.getOrNull()
                    }
                    patch.put("attendance", WidgetAttendanceSnapshot.make(slots, active, courses, receipts, before.optJSONObject("attendance")))
                }
                "calendar" -> {
                    val display = patch.getJSONObject("preferences").text("semesterDisplay")
                    val data = get("/api/calendar?refresh=1&semester=$display") as JSONObject
                    val todos = (get("/api/todos") as JSONArray).objects()
                    val courses = data.array("courses")
                    val courseIds = courses.map { it.optLong("id") }.toSet()
                    val palette = listOf("#ef4444", "#f97316", "#f5a50b", "#84cc16", "#22c55e", "#14b8a6", "#0ea5e9", "#3b82f6", "#6366f1", "#a855f7")
                    val colors = courses.groupBy { course ->
                        course.optJSONObject("term")?.let { "${it.optInt("year")}-${it.optInt("semester")}" }
                    }.values.flatMap { group ->
                        group.sortedWith(compareBy<JSONObject>({ it.text("code") }, { it.text("name") }, { it.optLong("id") }))
                            .mapIndexed { i, course ->
                                val index = if (group.size == 1) 7 else kotlin.math.floor(i * (palette.size - 1).toDouble() / (group.size - 1) + .5).toInt()
                                course.optLong("id") to palette[index]
                            }
                    }.toMap()
                    fun course(id: Long) = courses.firstOrNull { it.optLong("id") == id }?.text("name").orEmpty()
                    val statuses = mapOf("submitted" to "제출 완료", "not_submitted" to "미제출", "overdue" to "마감 지남", "done" to "출석 인정", "partial" to "부분 인정", "missed" to "미인정", "todo" to "미시청", "upcoming" to "시청 전")
                    val items = data.array("items").map { item ->
                        JSONObject().apply { listOf("key", "title", "due", "start", "done", "kind").forEach { put(it, item.opt(it) ?: JSONObject.NULL) } }
                            .put("course", course(item.optLong("courseId"))).put("color", colors[item.optLong("courseId")] ?: "#3b82f6").put("status", statuses[item.text("status")] ?: "상태 확인 필요")
                    } + todos.filter { display == "all" || it.isNull("courseId") || it.optLong("courseId") in courseIds }.map { todo ->
                        JSONObject().put("key", "todo:${todo.optLong("id")}").put("title", todo.text("title"))
                            .put("course", course(todo.optLong("courseId")).ifBlank { "공통" }).put("color", colors[todo.optLong("courseId")] ?: JSONObject.NULL)
                            .put("due", todo.opt("due") ?: JSONObject.NULL).put("allDay", todo.optBoolean("allDay"))
                            .put("done", !todo.isNull("doneAt")).put("kind", "todo").put("status", "")
                    }
                    patch.put("deadlines", JSONArray(items))
                    patch.put("semesterDisplay", display)
                    patch.put("slots", JSONArray(before.array("slots").map { slot -> slot.put("color", colors[courses.firstOrNull { sameCourse(it, slot) }?.optLong("id")] ?: slot.text("color", "#3b82f6")) }))
                }
                "seats" -> patch.put("seat", (get("/api/seats/session") as JSONObject).opt("session") ?: JSONObject.NULL)
                else -> patch.put("slots", JSONArray(colorSlots((get("/api/timetable?refresh=1") as JSONObject).array("slots"), before.array("slots"))))
            }
            updated.put(resource, System.currentTimeMillis())
            patch.put("updatedAt", updated)
            patch.put("errors", JSONObject().put(resource, ""))
        } catch (e: Exception) {
            if (e is WidgetFailure && e.status == 401) {
                if (WidgetData.read(context).text("owner") == owner) WidgetData.replace(context, "")
                return
            }
            val message = (e as? WidgetFailure)?.message ?: "정보를 불러오지 못했어요."
            patch.put("updatedAt", updated)
            patch.put("errors", JSONObject().put(resource, message))
            if (resource == "lectures") patch.put("attendance", (before.optJSONObject("attendance") ?: JSONObject()).put("error", message))
        } finally { lock.unlock() }
        WidgetData.merge(context, owner, patch)
    }
    private fun colorSlots(slots: List<JSONObject>, previous: List<JSONObject>) = slots.map { slot -> slot.put("color", previous.firstOrNull { sameCourse(it, slot) }?.text("color") ?: "#3b82f6") }
}

class WidgetJobService : JobService() {
    private val stopped = ConcurrentHashMap.newKeySet<Int>()
    override fun onStartJob(params: JobParameters): Boolean {
        stopped.remove(params.jobId)
        val requested = WidgetKind.entries.find { it.name == params.extras.getString("kind") }
        val kinds = if (requested == null) WidgetSync.installed(this).distinctBy { when { it.today || it == WidgetKind.WEEK -> "timetable"; it.deadlines -> "calendar"; else -> it.name } } else listOf(requested)
        kinds.forEach { Widgets.loading(this, it, true) }
        WidgetSync.worker.execute {
            try { for (kind in kinds) { if (stopped.contains(params.jobId)) break; WidgetSync.refresh(applicationContext, kind) } }
            finally { WidgetSync.main.post { kinds.forEach { Widgets.loading(this, it, false) }; Widgets.updateAll(this, preserveInput = true); if (!stopped.remove(params.jobId)) jobFinished(params, false) } }
        }
        return true
    }
    override fun onStopJob(params: JobParameters): Boolean {
        stopped.add(params.jobId)
        WidgetKind.entries.forEach { Widgets.loading(this, it, false) }
        return true
    }
}

class WidgetSystemReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        Widgets.updateAll(context, preserveInput = true)
        WidgetSync.schedule(context)
        WidgetSync.installed(context).forEach { WidgetSync.enqueue(context, it) }
    }
}
