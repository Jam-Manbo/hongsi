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
    if (optInt("status") !in 200..299) throw WidgetFailure(optInt("status"), optJSONObject("body")?.optJSONObject("error")?.text("message").orEmpty().ifBlank { "정보를 불러오지 못했어요" })
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
        fun get(path: String) = WidgetNative.api(context, path, owner = owner).body()
        try {
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
                    val data = get("/api/calendar?refresh=1") as JSONObject
                    val todos = (get("/api/todos") as JSONArray).objects()
                    val courses = data.array("courses")
                    val palette = listOf("#ef4444", "#f97316", "#f5a50b", "#84cc16", "#22c55e", "#14b8a6", "#0ea5e9", "#3b82f6", "#6366f1", "#a855f7")
                    val colors = courses.mapIndexed { i, c -> c.optLong("id") to palette[if (courses.size <= 1) 7 else if (courses.size > 10) i % 10 else kotlin.math.floor(i * 9.0 / (courses.size - 1) + .5).toInt()] }.toMap()
                    fun course(id: Long) = courses.firstOrNull { it.optLong("id") == id }?.text("name").orEmpty()
                    val statuses = mapOf("submitted" to "제출 완료", "draft" to "미제출", "not_submitted" to "미제출", "overdue" to "마감 지남", "done" to "출석 인정", "partial" to "부분 인정", "missed" to "미인정", "todo" to "미시청", "upcoming" to "시청 전")
                    val items = data.array("items").map { item ->
                        JSONObject().apply { listOf("key", "title", "due", "start", "done", "kind").forEach { put(it, item.opt(it) ?: JSONObject.NULL) } }
                            .put("course", course(item.optLong("courseId"))).put("color", colors[item.optLong("courseId")] ?: "#3b82f6").put("status", statuses[item.text("status")] ?: "상태 확인 필요")
                    } + todos.map { todo ->
                        JSONObject().put("key", "todo:${todo.optLong("id")}").put("title", todo.text("title"))
                            .put("course", course(todo.optLong("courseId")).ifBlank { "공통" }).put("color", colors[todo.optLong("courseId")] ?: JSONObject.NULL)
                            .put("dueAt", todo.opt("dueAt") ?: JSONObject.NULL).put("allDay", todo.optBoolean("allDay"))
                            .put("due", if (todo.isNull("dueAt")) JSONObject.NULL else todo.optLong("dueAt") + if (todo.optBoolean("allDay")) 86400 else 0)
                            .put("done", !todo.isNull("doneAt")).put("kind", "todo").put("status", "")
                    }
                    patch.put("deadlines", JSONArray(items))
                    patch.put("slots", JSONArray(before.array("slots").map { slot -> slot.put("color", colors[courses.firstOrNull { sameCourse(it, slot) }?.optLong("id")] ?: slot.text("color", "#3b82f6")) }))
                }
                "seats" -> patch.put("seat", (get("/api/seats/session") as JSONObject).opt("session") ?: JSONObject.NULL)
                else -> patch.put("slots", JSONArray(colorSlots((get("/api/timetable?refresh=1") as JSONObject).array("slots"), before.array("slots"))))
            }
            val updated = JSONObject().put(resource, System.currentTimeMillis())
            if (resource == "timetable") {
                runCatching { get("/api/preferences") as JSONObject }.onSuccess { preferences ->
                    val display = preferences.text("timetableDisplay")
                    if (display in listOf("full", "fit")) {
                        patch.put("preferences", JSONObject().put("timetableDisplay", display))
                        updated.put("preferences", preferences.optLong("updatedAt"))
                    }
                }.onFailure { if (it is WidgetFailure && it.status == 401) throw it }
            }
            patch.put("updatedAt", updated)
            patch.put("errors", JSONObject().put(resource, ""))
        } catch (e: Exception) {
            if (e is WidgetFailure && e.status == 401) {
                if (WidgetData.read(context).text("owner") == owner) WidgetData.replace(context, "")
                return
            }
            val message = (e as? WidgetFailure)?.message ?: "정보를 불러오지 못했어요"
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
