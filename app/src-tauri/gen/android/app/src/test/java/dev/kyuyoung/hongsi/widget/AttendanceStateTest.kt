package dev.kyuyoung.hongsi.widget

import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

class AttendanceStateTest {
    // Thursday 2026-10-08, 09:05 KST.
    private val now = 1791417900000L
    private val start = now - 5 * 60_000
    private fun mark(kind: String) = JSONObject().put("kind", kind).put("label", if (kind == "absent") "결석" else "출석")
    private fun data(age: Long = 30_000, kind: String? = "absent", open: Boolean = true): JSONObject {
        val slot = JSONObject().put("name", "테스트 수업").put("code", "TEST")
            .put("weekday", 3).put("start", "09:00").put("periods", JSONArray().put(1))
        val session = JSONObject(slot.toString()).put("at", start).put("identity", "session")
            .put("mark", kind?.let(::mark) ?: JSONObject.NULL)
        val lecture = JSONObject().put("name", "테스트 수업").put("code", "TEST").put("key", "test-key")
            .put("identity", "session").put("scheduleLabel", "목1 09:00 수업")
            .put("mark", kind?.let(::mark) ?: JSONObject.NULL)
        val snapshot = JSONObject().put("date", dateText(now / 1000)).put("checkedAt", now - age)
            .put("error", "").put("loaded", true).put("timetableLoaded", true)
            .put("sessions", JSONArray().put(session)).put("active", if (open) JSONArray().put(lecture) else JSONArray())
        return JSONObject().put("owner", "test-owner").put("slots", JSONArray().put(slot)).put("attendance", snapshot)
    }
    private fun read(data: JSONObject, state: JSONObject = JSONObject()) = AttendanceState.read(data, state, now)

    @Test fun openLectureSurvivesTwentySecondsAndOverridesAbsentHistory() {
        for (age in listOf(0L, 20_001L, 60_000L, 599_999L)) {
            val result = read(data(age))
            assertTrue("active response age=$age", result.available)
            assertEquals("available", result.kind)
        }
    }
    @Test fun openLectureSurvivesWithoutPublishedHistory() {
        assertTrue(read(data(kind = null)).available)
    }
    @Test fun newEmptyResponseClosesAttendanceImmediately() {
        assertFalse(read(data(age = 0, open = false)).available)
        assertEquals("absent", read(data(age = 0, open = false)).kind)
    }
    @Test fun oldOtherDayOrFutureResponsesDoNotOfferAttendance() {
        assertFalse(read(data(age = 600_000)).available)
        assertFalse(read(data(age = -1)).available)
        val yesterday = data().apply { getJSONObject("attendance").put("date", "2026-10-07") }
        assertFalse(read(yesterday).available)
    }
    @Test fun confirmedAttendanceAndLocalReceiptSuppressButton() {
        for (kind in listOf("present", "late", "excused")) assertFalse(read(data(kind = kind)).available)
        val state = JSONObject().put("localReceipt", JSONObject().put("identity", "session")
            .put("date", dateText(now / 1000)).put("kind", "present"))
        assertEquals("present", read(data(), state).kind)
        assertFalse(read(data(), state).available)
    }
    @Test fun transientFailureRetainsRecentOpenResultLikeApp() {
        assertTrue(read(data().apply { getJSONObject("attendance").put("error", "network") }).available)
    }
    @Test fun classTimeRefreshesEvenWithAbsentHistoryButNotEveryTick() {
        assertFalse(AttendanceState.needsRefresh(data(age = 59_999, open = false), now))
        assertTrue(AttendanceState.needsRefresh(data(age = 60_000, open = false), now))
        assertTrue(AttendanceState.needsRefresh(data(age = 600_000, open = false), now))
        assertFalse(AttendanceState.needsRefresh(data(age = 60_000, kind = "present", open = false), now))
    }
    @Test fun refreshUsesTodaysTimetableAfterDayChangeAndStopsOutsideClass() {
        val stale = data(age = 86400_000, open = false).apply { getJSONObject("attendance").put("date", "2026-10-07") }
        assertTrue(AttendanceState.needsRefresh(stale, now))
        assertFalse(AttendanceState.needsRefresh(stale, start - 180_001))
        assertFalse(AttendanceState.needsRefresh(stale, start + 3600_000))
        assertFalse(AttendanceState.needsRefresh(JSONObject(), now))
    }
    @Test fun recentlyOpenLectureRefreshesWithoutTimetable() {
        assertTrue(AttendanceState.needsRefresh(data(age = 60_000).put("slots", JSONArray()), now))
    }
    @Test fun expandedPeriodsAreCheckedSeparately() {
        val value = data(age = 60_000, kind = "present", open = false)
        value.getJSONArray("slots").getJSONObject(0).put("periods", JSONArray().put(1).put(2))
        assertTrue(AttendanceState.needsRefresh(value, start + 3600_000))
    }
    @Test fun slowerNativeResponseCannotOverwriteNewerAppAttendance() {
        val current = data(age = 0)
        val patch = JSONObject().put("attendance", data(age = 30_000, open = false).getJSONObject("attendance"))
            .put("updatedAt", JSONObject().put("lectures", now + 1000))
            .put("errors", JSONObject().put("lectures", "old failure"))
            .put("pendingReceipts", JSONArray())
        WidgetData.discardOlderAttendance(current, patch)
        assertFalse(patch.has("attendance"))
        assertFalse(patch.getJSONObject("updatedAt").has("lectures"))
        assertFalse(patch.getJSONObject("errors").has("lectures"))
        assertTrue(patch.has("pendingReceipts"))
    }
    @Test fun newerAndEqualResponsesCanUpdateAttendance() {
        for (age in listOf(0L, 10_000L)) {
            val patch = JSONObject().put("attendance", data(age = 0, open = false).getJSONObject("attendance"))
            WidgetData.discardOlderAttendance(data(age), patch)
            assertTrue(patch.has("attendance"))
        }
    }
}
