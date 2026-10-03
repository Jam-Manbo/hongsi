package dev.kyuyoung.hongsi.widget

import org.json.JSONArray
import org.json.JSONObject

internal fun sameCourse(a: JSONObject, b: JSONObject): Boolean {
    if (a.text("code").isNotBlank() && b.text("code").isNotBlank()) return a.text("code") == b.text("code")
    fun name(o: JSONObject) = o.text("name").replace(Regex("\\(\\*\\)|\\s"), "")
    return name(a) == name(b)
}
internal object WidgetAttendanceSnapshot {
    private val labels = mapOf("present" to "출석", "late" to "지각", "excused" to "공결", "absent" to "결석")
    fun valid(receipt: JSONObject) = receipt.text("date") == dateText() && receipt.text("kind") in listOf("present", "late", "excused") && receipt.optJSONObject("lecture")?.text("key")?.isNotBlank() == true && dateText(receipt.optLong("confirmedAt") / 1000) == dateText()
    private fun identity(item: JSONObject) = "${dateText()}|${item.text("code").ifBlank { item.text("name").replace(Regex("\\(\\*\\)|\\s"), "") }}|${if (item.has("at")) item.optLong("at").toString() else item.text("key", "course") }"
    fun make(slots: List<JSONObject>, active: List<JSONObject>, courses: List<JSONObject>, receipts: List<JSONObject>, previous: JSONObject?): JSONObject {
        val now = System.currentTimeMillis()
        val midnight = Math.floorDiv(now + 32400_000, 86400_000) * 86400_000 - 32400_000
        val sessions = slots.filter { it.optInt("weekday") == weekday() }.flatMap { slot ->
            val periods = slot.optJSONArray("periods") ?: JSONArray().put(1)
            (0 until periods.length()).map { i ->
                val start = minutes(slot.text("start")) + (periods.optInt(i) - periods.optInt(0)) * 60
                JSONObject(slot.toString()).put("start", hm(start)).put("at", midnight + start * 60_000L).put("round", i + 1)
            }
        }.sortedBy { it.optLong("at") }
        fun ref(lecture: JSONObject, at: Long = now): JSONObject {
            val time = Regex("(?:^|\\D)(\\d{1,2}):(\\d{2})").find(lecture.text("time"))?.let { "${it.groupValues[1].padStart(2, '0')}:${it.groupValues[2]}" }
            val matches = sessions.filter { sameCourse(it, lecture) && if (time != null) it.text("start") == time else at in (it.optLong("at") - 180_000)..(it.optLong("at") + 600_000) }
            return if (matches.size == 1) JSONObject(matches[0].toString()).put("key", lecture.text("key")) else JSONObject(lecture.toString())
        }
        fun mark(item: JSONObject): JSONObject? {
            val course = courses.filter { sameCourse(it, item) }.singleOrNull()
            if (course?.optBoolean("published") == true && item.has("at")) {
                val lessonSessions = sessions.filter { sameCourse(it, item) }
                val index = lessonSessions.indexOfFirst { it.optLong("at") == item.optLong("at") }
                val entries = course.array("weeks").flatMap { it.array("sessions") }.filter { it.text("date") == dateText(pattern = "M/d") }
                val entry = if (index >= 0 && entries.size == lessonSessions.size) entries[index] else null
                val label = entry?.let { labels[it.text("kind")] ?: it.text("mark").takeIf { _ -> it.text("kind") == "other" } }
                if (!label.isNullOrBlank()) return JSONObject().put("kind", entry!!.text("kind")).put("label", label).put("source", "school")
            }
            val receipt = receipts.filter { valid(it) }.sortedByDescending { it.optLong("confirmedAt") }.firstOrNull {
                val lecture = it.getJSONObject("lecture")
                (item.text("key").isNotBlank() && lecture.text("key") == item.text("key")) || identity(ref(lecture, it.optLong("confirmedAt"))) == identity(item)
            }
            return receipt?.let { JSONObject().put("kind", it.text("kind")).put("label", labels[it.text("kind")]).put("source", "app") }
        }
        val opened = active.map { lecture ->
            val item = ref(lecture)
            JSONObject(lecture.toString()).put("identity", identity(item)).put("mark", mark(item) ?: JSONObject.NULL)
        }
        val seen = (previous?.array("sessions").orEmpty().filter { it.optBoolean("seenOpen") }.map { it.text("identity") } + opened.map { it.text("identity") }).toSet()
        return JSONObject().put("date", dateText()).put("loaded", true).put("error", "").put("timetableLoaded", true).put("timetableError", "").put("checkedAt", now)
            .put("sessions", JSONArray(sessions.map { it.put("identity", identity(it)).put("mark", mark(it) ?: JSONObject.NULL).put("seenOpen", seen.contains(identity(it))) }))
            .put("active", JSONArray(opened))
    }
}
