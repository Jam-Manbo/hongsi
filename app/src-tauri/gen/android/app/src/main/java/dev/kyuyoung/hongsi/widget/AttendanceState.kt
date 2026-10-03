package dev.kyuyoung.hongsi.widget

import android.content.Context
import org.json.JSONObject

internal data class WidgetAttendance(
    val title: String = "", val message: String, val detail: String = "",
    val available: Boolean = false, val tone: String = "muted", val identity: String = "",
)

internal object AttendanceState {
    fun read(context: Context, state: JSONObject): WidgetAttendance {
        val data = WidgetData.read(context)
        if (data.length() == 0) return WidgetAttendance(message = "로그인이 필요해요", detail = "앱에서 로그인해 주세요")
        val snapshot = data.optJSONObject("attendance")
            ?: return WidgetAttendance(message = "출석 정보를 확인해 주세요", detail = "앱을 한 번 열어 주세요")
        val now = System.currentTimeMillis()
        val sessions = snapshot.array("sessions").takeIf { snapshot.text("date") == dateText() }.orEmpty()
        val current = sessions.firstOrNull { now in (it.optLong("at") - 180_000)..(it.optLong("at") + 600_000) }
        val next = sessions.firstOrNull { it.optLong("at") > now }
        val fresh = snapshot.text("error").isBlank() && snapshot.text("date") == dateText() && now - snapshot.optLong("checkedAt") in 0..20_000
        val active = if (fresh) snapshot.array("active") else emptyList()
        val open = active.firstOrNull { it.optJSONObject("mark")?.text("kind") !in listOf("present", "late", "excused") }
        if (open != null) {
            val mark = open.optJSONObject("mark")
            if (mark != null) return marked(open, mark)
            return withSuccess(state, WidgetAttendance(open.text("name"), "출석할 수 있어요", open.text("time"), true, "primary", open.text("identity")))
        }
        if (current != null) {
            current.optJSONObject("mark")?.let { return marked(current, it) }
            val base = WidgetAttendance(current.text("name"), "출석 가능 여부 확인 중", listOf(current.text("start") + " 수업", current.text("room")).filter(String::isNotBlank).joinToString("  "), identity = current.text("identity"))
            val confirmed = withSuccess(state, base)
            if (confirmed != base) return confirmed
            if (snapshot.text("error").isNotBlank()) return base.copy(message = "출석 정보를 불러오지 못했어요")
            if (!fresh) return base.copy(message = "출석 정보를 확인해 주세요", detail = "새로고침을 눌러 주세요")
            return base.copy(message = if (current.optBoolean("seenOpen") || now >= current.optLong("at")) "출석 확인 불가" else "출석 가능 여부 확인 중")
        }
        if (active.isNotEmpty()) {
            val item = active.first()
            item.optJSONObject("mark")?.let { return marked(item, it) }
        }
        if (snapshot.text("error").isNotBlank() || snapshot.text("timetableError").isNotBlank()) return WidgetAttendance(message = "출석 정보를 불러오지 못했어요")
        if (!snapshot.optBoolean("timetableLoaded") || !snapshot.optBoolean("loaded")) return WidgetAttendance(message = "출석 정보를 확인하고 있어요")
        if (snapshot.text("date") != dateText()) return WidgetAttendance(message = "출석 정보를 확인해 주세요", detail = "새로고침을 눌러 주세요")
        if (sessions.isEmpty()) return WidgetAttendance(message = "오늘은 수업이 없어요")
        if (next != null) return WidgetAttendance(next.text("name"), "다음 수업 ${next.text("start")}", next.text("room"))
        val inClass = sessions.lastOrNull { now in it.optLong("at") until (it.optLong("at") + 3600_000) }
        if (inClass != null) {
            inClass.optJSONObject("mark")?.let { return marked(inClass, it) }
            return withSuccess(state, WidgetAttendance(inClass.text("name"), "출석 확인 불가", "${inClass.text("start")} 수업", identity = inClass.text("identity")))
        }
        return WidgetAttendance(message = "오늘 수업이 모두 끝났어요")
    }
    private fun marked(item: JSONObject, mark: JSONObject): WidgetAttendance {
        val kind = mark.text("kind")
        val title = if (kind == "present") "출석 완료" else "${mark.text("label")} 처리됨"
        return WidgetAttendance(item.text("name"), title, item.text("start", item.text("time")), tone = when (kind) { "present", "excused" -> "success"; "absent" -> "error"; "late" -> "warn"; else -> "muted" }, identity = item.text("identity"))
    }
    private fun withSuccess(state: JSONObject, value: WidgetAttendance): WidgetAttendance {
        val receipt = state.optJSONObject("localReceipt") ?: return value
        return if (receipt.text("identity") == value.identity && receipt.text("date") == dateText()) value.copy(message = if (receipt.text("kind") == "late") "지각 처리됨" else if (receipt.text("kind") == "excused") "공결 처리됨" else "출석 완료", available = false, tone = if (receipt.text("kind") == "late") "warn" else "success") else value
    }
}
