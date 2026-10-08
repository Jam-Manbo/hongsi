package dev.kyuyoung.hongsi.widget

import android.content.Context
import android.icu.text.Collator
import android.icu.text.RuleBasedCollator
import android.util.AtomicFile
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.security.MessageDigest
import java.text.SimpleDateFormat
import java.util.Calendar
import java.util.Date
import java.util.Locale
import java.util.TimeZone

internal fun JSONArray?.objects(): List<JSONObject> = if (this == null) emptyList() else (0 until length()).mapNotNull { optJSONObject(it) }
internal fun JSONObject.text(key: String, fallback: String = ""): String = if (isNull(key)) fallback else optString(key, fallback)
internal fun JSONObject.array(key: String) = optJSONArray(key).objects()
internal fun nowSeconds() = System.currentTimeMillis() / 1000
internal val korea: TimeZone get() = TimeZone.getTimeZone("Asia/Seoul")
internal fun dateText(seconds: Long = nowSeconds(), pattern: String = "yyyy-MM-dd"): String = SimpleDateFormat(pattern, Locale.KOREAN).apply { timeZone = korea }.format(Date(seconds * 1000))
internal fun weekday(): Int = (Calendar.getInstance(korea).get(Calendar.DAY_OF_WEEK) + 5) % 7
internal fun minuteOfDay(): Int = Calendar.getInstance(korea).let { it.get(Calendar.HOUR_OF_DAY) * 60 + it.get(Calendar.MINUTE) }
internal fun minutes(value: String): Int = value.split(':').let { (it.getOrNull(0)?.toIntOrNull() ?: 0) * 60 + (it.getOrNull(1)?.toIntOrNull() ?: 0) }
internal fun hm(value: Int) = "%02d:%02d".format(value / 60, value % 60)

object WidgetData {
    private var cached: JSONObject? = null
    private fun file(context: Context) = AtomicFile(File(context.noBackupFilesDir, "widgets.json"))
    @Synchronized fun read(context: Context): JSONObject {
        if (cached == null) cached = runCatching { JSONObject(String(file(context).readFully(), Charsets.UTF_8)) }.getOrElse { JSONObject() }
        return JSONObject(cached!!.toString())
    }
    @Synchronized fun replace(context: Context, value: String) {
        require(value.length <= 2_000_000)
        val next = if (value.isBlank()) JSONObject() else JSONObject(value)
        if (next.length() > 0) require(next.optInt("version") == 1 && next.text("owner").isNotBlank())
        if (next.has("owner")) next.put("owner", ownerHash(next.getString("owner")))
        val current = read(context)
        if (current.text("owner") != next.text("owner")) context.getSharedPreferences("widgets-state", Context.MODE_PRIVATE).edit().clear().apply()
        else {
            val oldTimes = current.optJSONObject("updatedAt") ?: JSONObject()
            val times = next.optJSONObject("updatedAt") ?: JSONObject()
            if (oldTimes.optLong("preferences") > times.optLong("preferences")) {
                val preferences = next.optJSONObject("preferences") ?: JSONObject()
                preferences.put("timetableDisplay", current.optJSONObject("preferences")?.text("timetableDisplay", "fit") ?: "fit")
                preferences.put("semesterDisplay", current.optJSONObject("preferences")?.text("semesterDisplay", "current") ?: "current")
                next.put("preferences", preferences)
                times.put("preferences", oldTimes.optLong("preferences"))
            }
            val display = next.optJSONObject("preferences")?.text("semesterDisplay", "current") ?: "current"
            mapOf("slots" to "timetable", "deadlines" to "calendar", "seat" to "seats", "attendance" to "lectures").forEach { (field, resource) ->
                val matchingSemester = field != "deadlines" || current.text("semesterDisplay") == display
                if (matchingSemester && (oldTimes.optLong(resource) > times.optLong(resource) || (field == "deadlines" && next.text("semesterDisplay") != display))) {
                    if (current.has(field)) next.put(field, current.get(field))
                    if (field == "slots") next.put("timetableLoaded", hasTimetable(current))
                    if (field == "deadlines") next.put("semesterDisplay", current.text("semesterDisplay"))
                    times.put(resource, oldTimes.optLong(resource))
                }
            }
            next.put("updatedAt", times)
            next.put("pendingReceipts", current.optJSONArray("pendingReceipts") ?: JSONArray())
        }
        write(context, next)
    }
    fun ownerHash(value: String) = MessageDigest.getInstance("SHA-256").digest(value.toByteArray()).joinToString("") { "%02x".format(it) }
    @Synchronized fun merge(context: Context, owner: String, patch: JSONObject): Boolean {
        val next = read(context)
        if (owner.isBlank() || next.text("owner") != owner) return false
        val incomingPreferencesAt = patch.optJSONObject("updatedAt")?.optLong("preferences") ?: 0
        val preferencesAt = next.optJSONObject("updatedAt")?.optLong("preferences") ?: 0
        val display = next.optJSONObject("preferences")?.text("semesterDisplay", "current") ?: "current"
        if (patch.has("preferences") && (incomingPreferencesAt < preferencesAt ||
            (incomingPreferencesAt == preferencesAt && patch.getJSONObject("preferences").text("semesterDisplay", display) != display))) {
            patch.remove("preferences")
            patch.optJSONObject("updatedAt")?.remove("preferences")
        }
        val nextDisplay = patch.optJSONObject("preferences")?.text("semesterDisplay", display) ?: display
        if (patch.has("deadlines") && patch.text("semesterDisplay") != nextDisplay) return false
        patch.keys().forEach { key ->
            if (key in listOf("updatedAt", "errors", "preferences")) {
                val values = next.optJSONObject(key) ?: JSONObject()
                val changes = patch.getJSONObject(key)
                changes.keys().forEach { values.put(it, changes.get(it)) }
                next.put(key, values)
            } else next.put(key, patch.get(key))
        }
        write(context, next)
        return true
    }
    @Synchronized private fun write(context: Context, next: JSONObject) {
        val store = file(context)
        val stream = store.startWrite()
        try { stream.write(next.toString().toByteArray()); store.finishWrite(stream) }
        catch (e: Exception) { store.failWrite(stream); throw e }
        cached = next
    }
    fun state(context: Context, id: Int): JSONObject = runCatching {
        JSONObject(context.getSharedPreferences("widgets-state", Context.MODE_PRIVATE).getString("$id", "{}")!!)
    }.getOrElse { JSONObject() }
    fun save(context: Context, id: Int, state: JSONObject) { context.getSharedPreferences("widgets-state", Context.MODE_PRIVATE).edit().putString("$id", state.toString()).apply() }
    fun delete(context: Context, id: Int) { context.getSharedPreferences("widgets-state", Context.MODE_PRIVATE).edit().remove("$id").apply() }
    fun hasTimetable(data: JSONObject) = data.optBoolean("timetableLoaded")
        || (data.optJSONObject("updatedAt")?.optLong("timetable") ?: 0) > 0
        || data.array("slots").isNotEmpty()
    fun slots(context: Context) = read(context).array("slots")
    fun today(context: Context): List<JSONObject> = slots(context).filter { it.optInt("weekday") == weekday() }.sortedBy { it.text("start") }
    private fun previousMidnight(context: Context, due: Long) = read(context).optJSONObject("preferences")?.text("midnight", "prev") != "same" && dateText(due, "HH:mm") == "00:00"
    fun deadlineLabel(context: Context, due: Long, pattern: String = "M/d"): String {
        val previous = previousMidnight(context, due)
        return dateText(if (previous) due - 60 else due, pattern) + if (previous) " 24:00" else " " + dateText(due, "HH:mm")
    }
    fun deadlineDay(context: Context, due: Long) = Math.floorDiv(due + 32400 - if (previousMidnight(context, due)) 60 else 0, 86400)
    fun deadlineLabel(context: Context, item: JSONObject): String {
        if (item.isNull("due")) return "마감 없음"
        val due = item.optLong("due")
        if (item.optBoolean("allDay")) {
            val day = if (previousMidnight(context, due)) due - 60 else due
            return dateText(day, "M/d(E)") + " 하루 종일"
        }
        return deadlineLabel(context, due, "M/d(E)") + " 마감"
    }
    fun deadlineDay(context: Context, item: JSONObject): Long = deadlineDay(context, item.optLong("due"))
    fun deadlines(context: Context): List<JSONObject> {
        val data = read(context)
        if (data.text("semesterDisplay") != (data.optJSONObject("preferences")?.text("semesterDisplay", "current") ?: "current")) return emptyList()
        val now = nowSeconds()
        val titles = (Collator.getInstance(Locale.KOREAN) as RuleBasedCollator).apply { numericCollation = true }
        val items = data.array("deadlines").filter {
            !it.optBoolean("done") && !it.isNull("due") && it.optLong("due") > now
        }
        val midnight = data.optJSONObject("preferences")?.text("midnight", "prev") ?: "prev"
        val days = items.associateWith {
            val due = it.optLong("due")
            val previous = midnight != "same" && dateText(due, "HH:mm") == "00:00"
            Math.floorDiv(due + 32400 - if (previous) 60 else 0, 86400)
        }
        return items.sortedWith { a, b ->
            compareValues(days.getValue(a), days.getValue(b)).takeIf { it != 0 }
                ?: compareValues(a.optBoolean("allDay"), b.optBoolean("allDay")).takeIf { it != 0 }
                ?: compareValues(a.optLong("due"), b.optLong("due")).takeIf { it != 0 }
                ?: titles.compare(a.text("title"), b.text("title")).takeIf { it != 0 }
                ?: a.text("key").compareTo(b.text("key"))
        }
    }
    fun seat(context: Context): JSONObject? = read(context).optJSONObject("seat")?.takeIf { it.isNull("endedAt") }
    fun error(context: Context, resource: String) = read(context).optJSONObject("errors")?.text(resource).orEmpty()
}
