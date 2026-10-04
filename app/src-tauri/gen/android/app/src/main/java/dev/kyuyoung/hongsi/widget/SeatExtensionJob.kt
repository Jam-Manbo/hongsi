package dev.kyuyoung.hongsi.widget

import android.app.job.JobInfo
import android.app.job.JobParameters
import android.app.job.JobScheduler
import android.app.job.JobService
import android.content.ComponentName
import android.content.Context
import android.os.PersistableBundle
import android.widget.Toast
import app.tauri.notification.NotificationStorage
import app.tauri.notification.NotificationSchedule
import app.tauri.notification.TauriNotificationManager
import com.fasterxml.jackson.databind.ObjectMapper
import com.fasterxml.jackson.databind.DeserializationFeature
import com.fasterxml.jackson.databind.module.SimpleModule
import com.fasterxml.jackson.databind.JsonDeserializer
import com.fasterxml.jackson.databind.JsonNode
import com.fasterxml.jackson.databind.DeserializationContext
import com.fasterxml.jackson.core.JsonParser
import app.tauri.plugin.JSObject
import org.json.JSONObject
import java.util.Date
import java.util.UUID
import java.util.concurrent.locks.ReentrantLock
import kotlin.concurrent.withLock

class SeatExtensionJob : JobService() {
    companion object {
        private const val JOB = 7490
        @Volatile private var running = false
        fun pending(context: Context) = running || context.getSystemService(JobScheduler::class.java).getPendingJob(JOB) != null
        internal fun enqueue(context: Context, operation: String) {
            if (pending(context)) return
            val fields = operation.split(':')
            if (fields.size != 4 || fields[1].isBlank() || fields[1] != WidgetData.read(context).text("owner")) return
            val seat = fields[2].toLongOrNull()?.takeIf { it > 0 } ?: return
            val count = fields[3].toIntOrNull() ?: return
            val extras = PersistableBundle().apply {
                putString("owner", fields[1]); putLong("seat", seat); putInt("count", count)
                putLong("created", System.currentTimeMillis()); putString("request", UUID.randomUUID().toString())
            }
            val job = JobInfo.Builder(JOB, ComponentName(context, SeatExtensionJob::class.java)).setExtras(extras).setOverrideDeadline(0).build()
            val accepted = context.getSystemService(JobScheduler::class.java).schedule(job) == JobScheduler.RESULT_SUCCESS
            if (!accepted) result(context, "연장을 시작하지 못했어요. 잠시 후 다시 시도해 주세요.")
            Widgets.updateAll(context, preserveInput = true)
        }
        internal fun result(context: Context, message: String) {
            Toast.makeText(context, message, Toast.LENGTH_SHORT).show()
        }
        internal fun moveReminders(context: Context, owner: String, before: JSONObject, after: JSONObject) {
            val mapper = ObjectMapper().disable(DeserializationFeature.FAIL_ON_UNKNOWN_PROPERTIES).registerModule(
                SimpleModule().addDeserializer(JSObject::class.java, object : JsonDeserializer<JSObject>() {
                    override fun deserialize(parser: JsonParser, context: DeserializationContext): JSObject = JSObject(parser.codec.readTree<JsonNode>(parser).toString())
                })
            )
            val storage = NotificationStorage(context, mapper)
            val manager = TauriNotificationManager(storage, null, context, null)
            val shift = (after.optLong("expiresAt") - before.optLong("expiresAt")) * 1000
            for (id in storage.getSavedNotificationIds().filter { (it.toIntOrNull() ?: 0) in 7000..7499 }) {
                if (WidgetData.read(context).text("owner") != owner) return
                val notification = storage.getSavedNotification(id) ?: error("예약된 퇴실 알림을 읽지 못했어요")
                val intent = runCatching { JSONObject(notification.extra?.optString("intent").orEmpty()) }.getOrNull() ?: continue
                if (WidgetData.ownerHash(intent.text("account")) != owner || intent.optJSONObject("target")?.optLong("id") != before.optLong("id")) continue
                val schedule = notification.schedule as? NotificationSchedule.At ?: continue
                manager.cancel(listOf(notification.id))
                schedule.date = Date(schedule.date.time + shift)
                if (schedule.date.time <= System.currentTimeMillis()) continue
                intent.put("at", schedule.date.time)
                notification.extra?.put("intent", intent.toString())
                notification.body = notification.body?.replace(dateText(before.optLong("expiresAt"), "HH:mm"), dateText(after.optLong("expiresAt"), "HH:mm"))
                val at = java.text.SimpleDateFormat("yyyy-MM-dd'T'HH:mm:ss.SSS'Z'", java.util.Locale.US).apply { timeZone = java.util.TimeZone.getTimeZone("UTC") }.format(schedule.date)
                notification.sourceJson = JSONObject().put("id", notification.id).put("title", notification.title).put("body", notification.body)
                    .put("icon", "ic_notification").put("iconColor", "#FF7A3D").put("autoCancel", true).put("extra", notification.extra)
                    .put("schedule", JSONObject().put("at", JSONObject().put("date", at).put("repeating", false).put("allowWhileIdle", true))).toString()
                manager.schedule(notification)
                storage.appendNotifications(listOf(notification))
            }
        }
    }
    @Volatile private var stopped = false
    override fun onStartJob(params: JobParameters): Boolean {
        running = true
        stopped = false
        val context = applicationContext
        val extras = params.extras
        val owner = extras.getString("owner").orEmpty()
        Widgets.updateAll(context, preserveInput = true)
        WidgetSync.worker.execute {
            val outcome = runCatching {
                WidgetSync.locks.getOrPut("seats") { ReentrantLock() }.withLock {
                    if (owner.isBlank() || WidgetData.read(context).text("owner") != owner) throw WidgetFailure(409, "계정이 변경됐어요. 다시 확인해 주세요.")
                    if (System.currentTimeMillis() - extras.getLong("created") !in 0..120_000) throw WidgetFailure(408, "요청 시간이 지났어요. 다시 눌러 주세요.")
                    val marker = context.getSharedPreferences("widget-seat-actions", Context.MODE_PRIVATE)
                    val request = extras.getString("request") ?: throw WidgetFailure(400, "다시 시도해 주세요.")
                    if (marker.getString("claimed", null) == request) throw WidgetFailure(409, "처리 결과를 새로고침해서 확인해 주세요.")
                    if (!marker.edit().putString("claimed", request).commit()) throw WidgetFailure(500, "잠시 후 다시 시도해 주세요.")
                    val current = (WidgetNative.api(context, "/api/seats/session", owner = owner).body() as JSONObject).optJSONObject("session")
                    if (current == null || current.optLong("id") != extras.getLong("seat") || current.optInt("extendCount") != extras.getInt("count")) throw WidgetFailure(409, "좌석 정보가 변경됐어요. 새로고침해 주세요.")
                    if (stopped || WidgetData.read(context).text("owner") != owner) throw WidgetFailure(409, "요청이 중단됐어요. 다시 확인해 주세요.")
                    val response = WidgetNative.api(context, "/api/seats/session/extend", "POST", owner = owner).body() as JSONObject
                    val seat = response.optJSONObject("session") ?: throw WidgetFailure(500, "처리 결과를 새로고침해서 확인해 주세요.")
                    if (seat.optLong("id") != current.optLong("id")) throw WidgetFailure(409, "좌석 정보가 변경됐어요. 새로고침해 주세요.")
                    val applied = WidgetData.merge(context, owner, JSONObject().put("seat", seat).put("updatedAt", JSONObject().put("seats", System.currentTimeMillis())).put("errors", JSONObject().put("seats", "")))
                    if (!applied) throw WidgetFailure(409, "계정이 변경됐어요.")
                    val moved = runCatching { moveReminders(context, owner, current, seat) }.isSuccess
                    if (moved) "이용 시간을 연장했어요." else "연장했지만 퇴실 알림을 갱신하지 못했어요."
                }
            }
            WidgetSync.main.post {
                running = false
                if (!stopped) jobFinished(params, false)
                if (WidgetData.read(context).text("owner") == owner) {
                    outcome.onSuccess { result(context, it) }
                        .onFailure { result(context, (it as? WidgetFailure)?.message ?: "처리 결과를 새로고침해서 확인해 주세요.") }
                    WidgetNavigation.changed = true
                    WidgetSync.enqueue(context, WidgetKind.SEAT)
                }
                Widgets.updateAll(context, preserveInput = true)
            }
        }
        return true
    }
    override fun onStopJob(params: JobParameters): Boolean {
        stopped = true
        return false
    }
}
