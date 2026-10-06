package dev.kyuyoung.hongsi.widget

import android.content.Context
import org.json.JSONObject

internal data class WidgetAttendance(
    val title: String = "", val message: String, val detail: String = "",
    val available: Boolean = false, val tone: String = "muted", val identity: String = "", val kind: String = "",
)

internal object AttendanceState {
    fun schedule(item: JSONObject, sessions: List<JSONObject> = emptyList()): String {
        item.text("scheduleLabel").takeIf(String::isNotBlank)?.let { return it }
        val matching = sessions.filter { sameCourse(it, item) }
        val clock = Regex("(?:^|\\D)(\\d{1,2}):(\\d{2})").find(item.text("time"))?.let { "${it.groupValues[1].padStart(2, '0')}:${it.groupValues[2]}" }
        val period = Regex("^\\s*([월화수목금토일])\\s*(\\d{1,2})(?:교시)?\\s*$").matchEntire(item.text("time"))
        val now = System.currentTimeMillis()
        val candidates = matching.filter {
            when {
                clock != null -> it.text("start") == clock
                period != null -> it.optInt("weekday", -1) == "월화수목금토일".indexOf(period.groupValues[1]) && it.optJSONArray("periods")?.optInt(0) == period.groupValues[2].toIntOrNull()
                else -> now in (it.optLong("at") - 180_000)..(it.optLong("at") + 600_000)
            }
        }
        val slot = if (item.has("weekday") && item.has("periods")) item else candidates.singleOrNull()
            ?: matching.singleOrNull().takeIf { clock == null && period == null }
            ?: return item.text("time")
        val day = "월화수목금토일".getOrNull(slot.optInt("weekday", -1))?.toString().orEmpty()
        val periods = slot.optJSONArray("periods")
        val index = if ((periods?.length() ?: 0) > 1 && slot.has("round")) (slot.optInt("round") - 1).coerceAtLeast(0) else 0
        val number = periods?.optInt(index)?.takeIf { it > 0 }?.toString().orEmpty()
        val start = slot.text("start").takeIf(String::isNotBlank)?.let { "$it 수업" }.orEmpty()
        return listOf(day + number, start, slot.text("room")).filter(String::isNotBlank).joinToString(" ")
    }
    fun pending(context: Context, state: JSONObject): WidgetAttendance {
        val lecture = state.optJSONObject("lecture") ?: JSONObject()
        val sessions = WidgetData.read(context).optJSONObject("attendance")?.array("sessions").orEmpty()
        return WidgetAttendance(lecture.text("name"), "출석을 확인하고 있어요.", schedule(lecture, sessions), tone = "primary", kind = "pending")
    }
    fun read(context: Context, state: JSONObject): WidgetAttendance {
        val data = WidgetData.read(context)
        if (data.length() == 0) return WidgetAttendance(message = "로그인이 필요해요.", detail = "앱에서 로그인해 주세요.")
        val snapshot = data.optJSONObject("attendance")
            ?: return WidgetAttendance(message = "출석 정보를 확인해 주세요.", detail = "앱을 한 번 열어 주세요.")
        val now = System.currentTimeMillis()
        val sessions = snapshot.array("sessions").takeIf { snapshot.text("date") == dateText() }.orEmpty()
        val current = sessions.firstOrNull { now in (it.optLong("at") - 180_000)..(it.optLong("at") + 600_000) }
        val next = sessions.firstOrNull { it.optLong("at") > now }
        val fresh = snapshot.text("error").isBlank() && snapshot.text("date") == dateText() && now - snapshot.optLong("checkedAt") in 0..20_000
        val active = if (fresh) snapshot.array("active") else emptyList()
        val open = active.firstOrNull { it.optJSONObject("mark")?.text("kind") !in listOf("present", "late", "excused") }
        if (open != null) {
            val mark = open.optJSONObject("mark")
            if (mark != null) return marked(open, mark, sessions)
            return withSuccess(state, WidgetAttendance(open.text("name"), "출석할 수 있어요.", schedule(open, sessions), true, "primary", open.text("identity"), "available"))
        }
        if (current != null) {
            current.optJSONObject("mark")?.let { return marked(current, it) }
            val base = WidgetAttendance(current.text("name"), "출석 가능 여부 확인 중", schedule(current), tone = "primary", identity = current.text("identity"), kind = "waiting")
            val confirmed = withSuccess(state, base)
            if (confirmed != base) return confirmed
            if (snapshot.text("error").isNotBlank()) return base.copy(message = "출석 정보를 불러오지 못했어요.", tone = "error", kind = "error")
            if (!fresh) return base.copy(message = "출석 정보를 확인해 주세요.", tone = "warn", kind = "stale")
            return if (current.optBoolean("seenOpen") || now >= current.optLong("at")) base.copy(message = "출석 확인 불가", tone = "warn", kind = "unknown") else base
        }
        if (active.isNotEmpty()) {
            val item = active.first()
            item.optJSONObject("mark")?.let { return marked(item, it, sessions) }
        }
        if (snapshot.text("error").isNotBlank() || snapshot.text("timetableError").isNotBlank()) return WidgetAttendance(message = "출석 정보를 불러오지 못했어요.", tone = "error", kind = "error")
        if (!snapshot.optBoolean("timetableLoaded") || !snapshot.optBoolean("loaded")) return WidgetAttendance(message = "출석 정보를 확인하고 있어요.", kind = "loading")
        if (snapshot.text("date") != dateText()) return WidgetAttendance(message = "출석 정보를 확인해 주세요.", detail = "새로고침을 눌러 주세요.")
        if (sessions.isEmpty()) return WidgetAttendance(message = "오늘은 수업이 없어요.")
        if (next != null) return WidgetAttendance(next.text("name"), "다음 수업", schedule(next), kind = "next")
        val inClass = sessions.lastOrNull { now in it.optLong("at") until (it.optLong("at") + 3600_000) }
        if (inClass != null) {
            inClass.optJSONObject("mark")?.let { return marked(inClass, it) }
            return withSuccess(state, WidgetAttendance(inClass.text("name"), "출석 확인 불가", schedule(inClass), tone = "warn", identity = inClass.text("identity"), kind = "unknown"))
        }
        return WidgetAttendance(message = "오늘 수업이 모두 끝났어요.")
    }
    private fun marked(item: JSONObject, mark: JSONObject, sessions: List<JSONObject> = emptyList()): WidgetAttendance {
        val kind = mark.text("kind")
        val title = if (kind == "present") "출석 완료" else "${mark.text("label")} 처리됨"
        return WidgetAttendance(item.text("name"), title, schedule(item, sessions), tone = when (kind) { "present", "excused" -> "success"; "absent" -> "error"; "late" -> "warn"; else -> "muted" }, identity = item.text("identity"), kind = kind)
    }
    private fun withSuccess(state: JSONObject, value: WidgetAttendance): WidgetAttendance {
        val receipt = state.optJSONObject("localReceipt") ?: return value
        return if (receipt.text("identity") == value.identity && receipt.text("date") == dateText()) value.copy(message = if (receipt.text("kind") == "late") "지각 처리됨" else if (receipt.text("kind") == "excused") "공결 처리됨" else "출석 완료", available = false, tone = if (receipt.text("kind") == "late") "warn" else "success", kind = receipt.text("kind", "present")) else value
    }
}
