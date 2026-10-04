package dev.kyuyoung.hongsi.widget

import android.app.PendingIntent
import android.appwidget.AppWidgetManager
import android.appwidget.AppWidgetProvider
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import dev.kyuyoung.hongsi.R
import org.json.JSONObject

internal enum class WidgetKind(val title: String, val height: Int, val receiver: Class<out HongsiWidgetProvider>) {
    SEAT("열람실 좌석", 192, SeatWidgetReceiver::class.java),
    ATTENDANCE("빠른 출결", 192, AttendanceWidgetReceiver::class.java),
    DEADLINES("다가오는 마감", 204, DeadlinesWidgetReceiver::class.java),
    DEADLINES_LARGE("다가오는 마감 크게", 396, DeadlinesLargeWidgetReceiver::class.java),
    TODAY("오늘 수업", 204, TodayWidgetReceiver::class.java),
    TODAY_LARGE("오늘 수업 크게", 396, TodayLargeWidgetReceiver::class.java),
    WEEK("주간 시간표", 396, WeekWidgetReceiver::class.java);
    val today get() = this == TODAY || this == TODAY_LARGE
    val attendance get() = this == ATTENDANCE
    val deadlines get() = this == DEADLINES || this == DEADLINES_LARGE

}

open class HongsiWidgetProvider : AppWidgetProvider() {
    internal val kind get() = WidgetKind.entries.first { it.receiver == javaClass }
    override fun onUpdate(context: Context, manager: AppWidgetManager, ids: IntArray) { ids.forEach { Widgets.update(context, it, kind) }; WidgetSync.enqueue(context, kind) }
    override fun onAppWidgetOptionsChanged(context: Context, manager: AppWidgetManager, id: Int, options: Bundle) { Widgets.update(context, id, kind) }
    override fun onDeleted(context: Context, ids: IntArray) { ids.forEach { WidgetData.delete(context, it) }; Widgets.reschedule(context, kind) }
    override fun onRestored(context: Context, oldIds: IntArray, newIds: IntArray) {
        oldIds.zip(newIds).forEach { (old, new) -> WidgetData.save(context, new, WidgetData.state(context, old)); WidgetData.delete(context, old) }
        onUpdate(context, AppWidgetManager.getInstance(context), newIds)
    }
    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action == Widgets.ACTION) {
            val id = intent.getIntExtra("widget", AppWidgetManager.INVALID_APPWIDGET_ID)
            if (id <= 0) return
            if ( AppWidgetManager.getInstance(context).getAppWidgetInfo(id)?.provider != ComponentName(context, javaClass)) return
            if (id == AppWidgetManager.INVALID_APPWIDGET_ID) return
            val operation = intent.getStringExtra("operation").orEmpty()
            Widgets.act(context, id, kind, operation)
        } else if (intent.action == Widgets.TICK) {
            Widgets.ids(context, kind).forEach { Widgets.update(context, it, kind) }
        } else super.onReceive(context, intent)
    }
}
class AttendanceWidgetReceiver : HongsiWidgetProvider()
class DeadlinesLargeWidgetReceiver : HongsiWidgetProvider()
class TodayWidgetReceiver : HongsiWidgetProvider()
class TodayLargeWidgetReceiver : HongsiWidgetProvider()
class DeadlinesWidgetReceiver : HongsiWidgetProvider()
class SeatWidgetReceiver : HongsiWidgetProvider()
class WeekWidgetReceiver : HongsiWidgetProvider()

internal object Widgets {
    const val TICK = "dev.kyuyoung.hongsi.WIDGET_LOCAL_TICK"
    const val ACTION = "dev.kyuyoung.hongsi.WIDGET_ACTION"
    private val inputHandler = Handler(Looper.getMainLooper())
    private val loading = mutableMapOf<WidgetKind, Long>()
    fun refreshing(kind: WidgetKind) = System.currentTimeMillis() - (loading[kind] ?: 0) < 120_000
    fun loading(context: Context, kind: WidgetKind, active: Boolean) {
        if (active) loading[kind] = System.currentTimeMillis() else loading.remove(kind)
        ids(context, kind).forEach { update(context, it, kind) }
    }
    fun ids(context: Context, kind: WidgetKind) = AppWidgetManager.getInstance(context).getAppWidgetIds(ComponentName(context, kind.receiver))
    private val inputRenders = mutableMapOf<Int, Runnable>()
    private fun cancelInputRender(id: Int) { inputRenders.remove(id)?.let(inputHandler::removeCallbacks) }
    fun pending(context: Context, id: Int, kind: WidgetKind, operation: String): PendingIntent = PendingIntent.getBroadcast(context, 0,
        Intent(context, kind.receiver).setAction(ACTION).setData(Uri.parse("hongsi-widget://action/$id/${Uri.encode(operation)}"))
            .putExtra("widget", id).putExtra("operation", operation).addFlags(Intent.FLAG_RECEIVER_FOREGROUND),
        PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
    fun page(context: Context, id: Int, kind: WidgetKind, detail: String = ""): PendingIntent = PendingIntent.getActivity(context, 0,
        Intent(context, dev.kyuyoung.hongsi.MainActivity::class.java).setAction("dev.kyuyoung.hongsi.WIDGET_OPEN").setData(Uri.parse("hongsi-widget://open/$id/${Uri.encode(detail)}"))
            .putExtra("widget", id).putExtra("kind", kind.name).putExtra("detail", detail).putExtra("owner", WidgetData.read(context).text("owner")).addFlags(Intent.FLAG_ACTIVITY_CLEAR_TOP or Intent.FLAG_ACTIVITY_SINGLE_TOP),
        PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
    fun update(context: Context, id: Int, kind: WidgetKind, partial: Boolean = false) {
        if (!partial) cancelInputRender(id)
        if (id <= 0) return
        val manager = AppWidgetManager.getInstance(context)
        if (partial) {
            @Suppress("DEPRECATION")
            manager.notifyAppWidgetViewDataChanged(id, R.id.widget_code_host)
        } else manager.updateAppWidget(id, WidgetViews.render(context, id, kind))
        if (!partial) schedule(context, kind)
    }
    fun reschedule(context: Context, kind: WidgetKind) { schedule(context, kind); WidgetSync.schedule(context) }
    private fun schedule(context: Context, kind: WidgetKind) {
        val manager = AppWidgetManager.getInstance(context)
        val ids = manager.getAppWidgetIds(ComponentName(context, kind.receiver))
        val alarm = context.getSystemService(android.app.AlarmManager::class.java)
        val pending = PendingIntent.getBroadcast(context, 0, Intent(context, kind.receiver).setAction(TICK), PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
        val active = when {
            kind == WidgetKind.SEAT -> ids.any { WidgetData.seat(context, WidgetData.state(context, it)) != null }
            kind.attendance || kind.today || kind.deadlines || kind == WidgetKind.WEEK -> ids.isNotEmpty()
            else -> false
        }
        if (active) alarm.set(android.app.AlarmManager.RTC, (System.currentTimeMillis() / 60_000 + 1) * 60_000, pending)
        else alarm.cancel(pending)
    }
    fun updateAll(context: Context, preserveInput: Boolean = false) {
        val manager = AppWidgetManager.getInstance(context)
        WidgetKind.entries.forEach { kind ->
            manager.getAppWidgetIds(ComponentName(context, kind.receiver)).forEach { id ->
                if (!preserveInput || WidgetData.state(context, id).text("mode", "ready") != "input" || WidgetData.read(context).length() == 0) update(context, id, kind)
            }
        }
    }
    fun act(context: Context, id: Int, kind: WidgetKind, operation: String) {
        if (kind == WidgetKind.SEAT && operation.startsWith("extend:")) {
            SeatExtensionJob.enqueue(context, operation)
            return
        }
        if (kind.attendance && WidgetAttendanceService.pending(context)) return
        val state = WidgetData.state(context, id)
        val code = state.text("code")
        val input = state.text("mode") == "input"
        var partial = false
        when {
            operation.startsWith("digit:") && input && code.length < 4 -> {
                val digit = operation.substringAfter(':')
                if (digit.length != 1 || digit[0] !in '0'..'9') return
                state.put("code", code + digit); partial = true
            }
            operation == "erase" && input -> { state.put("code", code.dropLast(1)); partial = true }
            operation == "open" -> {
                val attendance = AttendanceState.read(context, state)
                if (!attendance.available) { WidgetSync.enqueue(context, kind); return }
                val lecture = WidgetData.read(context).optJSONObject("attendance")?.array("active")?.firstOrNull { it.text("identity") == attendance.identity }
                if (lecture == null || lecture.text("key").isBlank()) { WidgetSync.enqueue(context, kind); return }
                state.put("mode", "input").put("code", "").put("lecture", lecture).put("inputAt", System.currentTimeMillis())
            }
            operation == "back" -> state.put("mode", "ready").put("code", "")
            operation == "refresh" -> {
                if (!refreshing(kind)) { loading(context, kind, true); WidgetSync.enqueue(context, kind) }
                return
            }
            else -> return
        }
        WidgetData.save(context, id, state)
        if (partial) {
            cancelInputRender(id)
            val render = Runnable { inputRenders.remove(id); update(context, id, kind, partial = true) }
            inputRenders[id] = render
            inputHandler.postDelayed(render, 50)
        } else update(context, id, kind)
    }
}
