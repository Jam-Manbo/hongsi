import Foundation

// Run with: bash app/scripts/test-ios-attendance.sh
@main
enum AttendanceWidgetTests {
    static let now = Date(timeIntervalSince1970: 1_791_417_900) // 2026-10-08 09:05 KST
    static let start = now.addingTimeInterval(-5 * 60)
    static func fixture(age: Double = 30_000, kind: String? = "absent", open: Bool = true) -> JSON {
        let mark: Any = kind.map { ["kind": $0, "label": $0 == "absent" ? "결석" : "출석"] } as Any? ?? NSNull()
        let slot: JSON = ["name": "테스트 수업", "code": "TEST", "weekday": 3, "start": "09:00", "periods": [1]]
        var session = slot
        session["at"] = HongsiClock.milliseconds(start); session["identity"] = "session"; session["mark"] = mark
        let lecture: JSON = ["name": "테스트 수업", "code": "TEST", "key": "test-key", "identity": "session", "scheduleLabel": "목1 09:00 수업", "mark": mark]
        let snapshot: JSON = ["date": HongsiClock.text(now), "checkedAt": HongsiClock.milliseconds(now) - age,
                              "error": "", "loaded": true, "timetableLoaded": true, "sessions": [session], "active": open ? [lecture] : []]
        return ["owner": "test-owner", "slots": [slot], "attendance": snapshot]
    }
    static func changeSnapshot(_ data: JSON, _ change: (inout JSON) -> Void) -> JSON {
        var value = data, snapshot = data.object("attendance"); change(&snapshot); value["attendance"] = snapshot; return value
    }
    static func status(_ data: JSON, state: JSON = [:], at: Date = now) -> AttendanceStatus {
        WidgetModel.attendance(data, state: state, now: at)
    }
    struct Failure: Error, CustomStringConvertible {
        let description: String
    }
    static func expect(_ condition: @autoclosure () -> Bool, _ message: String) throws {
        if !condition() { throw Failure(description: message) }
    }
    static func main() throws {
        let tests: [(String, () throws -> Void)] = [
            ("open lecture overrides absent history beyond twenty seconds", {
                for age in [0.0, 20_001, 60_000, 599_999] {
                    try expect(status(fixture(age: age)).available, "Open attendance disappeared at age \(age)")
                }
            }),
            ("open lecture without history remains available", {
                try expect(status(fixture(kind: nil)).available, "Unpublished history hid open attendance")
            }),
            ("new empty response closes attendance", {
                let value = status(fixture(age: 0, open: false))
                try expect(!value.available && value.kind == "absent", "Closed attendance stayed available")
            }),
            ("expired, future and previous-day responses stay closed", {
                try expect(!status(fixture(age: 600_000)).available, "Expired attendance stayed available")
                try expect(!status(fixture(age: -1)).available, "Future response was accepted")
                let old = changeSnapshot(fixture()) { $0["date"] = "2026-10-07" }
                try expect(!status(old).available, "Previous-day attendance was accepted")
            }),
            ("confirmed attendance and local receipt suppress action", {
                for kind in ["present", "late", "excused"] {
                    try expect(!status(fixture(kind: kind)).available, "Confirmed \(kind) offered attendance")
                }
                let state: JSON = ["localReceipt": ["identity": "session", "date": HongsiClock.text(now), "kind": "present"]]
                let value = status(fixture(), state: state)
                try expect(!value.available && value.kind == "present", "Local confirmation was ignored")
            }),
            ("transient failure preserves a recent open response", {
                let value = changeSnapshot(fixture()) { $0["error"] = "network" }
                try expect(status(value).available, "Transient error hid recent attendance")
            }),
            ("class refresh remains frequent with absent history", {
                for kind in [nil, "absent"] as [String?] {
                    let date = WidgetModel.attendanceReloadDate(fixture(kind: kind, open: false), now: now)
                    try expect(date == now.addingTimeInterval(300), "Class refresh did not request five minutes")
                }
                let date = WidgetModel.attendanceReloadDate(fixture(kind: "present", open: false), now: now)
                try expect(date == now.addingTimeInterval(900), "Completed class still refreshed frequently")
            }),
            ("upcoming class advances reload to watch window", {
                let before = start.addingTimeInterval(-10 * 60)
                let date = WidgetModel.attendanceReloadDate(fixture(age: 3_600_000, open: false), now: before)
                try expect(date == start.addingTimeInterval(-180), "Reload missed upcoming class window")
                let outside = start.addingTimeInterval(3_600)
                try expect(WidgetModel.attendanceReloadDate(fixture(open: false), now: outside) == outside.addingTimeInterval(900), "Finished class kept frequent reloads")
                try expect(WidgetModel.attendanceReloadDate([:], now: now) == now.addingTimeInterval(900), "Logged-out data requested frequent reloads")
            }),
            ("day rollover uses timetable and ignores yesterday's completion", {
                let stale = changeSnapshot(fixture(age: 86_400_000, kind: "present", open: false)) { $0["date"] = "2026-10-07" }
                try expect(WidgetModel.attendanceReloadDate(stale, now: now) == now.addingTimeInterval(300), "Yesterday's mark blocked today's refresh")
            }),
            ("open lecture refreshes without timetable", {
                var data = fixture(); data["slots"] = [] as [JSON]
                try expect(WidgetModel.attendanceReloadDate(data, now: now) == now.addingTimeInterval(300), "Open class without timetable did not refresh")
            }),
            ("second period refreshes after first period is confirmed", {
                var data = fixture(kind: "present", open: false), slots = data.objects("slots")
                slots[0]["periods"] = [1, 2]; data["slots"] = slots
                let second = start.addingTimeInterval(3_600)
                try expect(WidgetModel.attendanceReloadDate(data, now: second) == second.addingTimeInterval(300), "First-period mark blocked second period")
            }),
            ("timeline preserves open state until cache expires", {
                let data = fixture(age: 0)
                let dates = WidgetModel.attendanceTimelineDates(data, state: [:], now: now)
                try expect(!dates.contains(now.addingTimeInterval(21)), "Old 21-second transition remains")
                try expect(dates.contains(now.addingTimeInterval(600)), "Cache expiration transition is missing")
                for date in dates where date < now.addingTimeInterval(600) {
                    try expect(status(data, at: date).available, "A timeline entry hid open attendance too early")
                }
                try expect(!status(data, at: now.addingTimeInterval(600)).available, "Cache expiration did not hide attendance")
                let state: JSON = ["inputAt": HongsiClock.milliseconds(now) - 10_000, "pendingAt": HongsiClock.milliseconds(now)]
                let inputDates = WidgetModel.attendanceTimelineDates(data, state: state, now: now)
                try expect(inputDates.contains(now.addingTimeInterval(590)), "Input expiration lost")
                try expect(inputDates.contains(now.addingTimeInterval(45)), "Pending expiration lost")
            }),
            ("slow native response cannot replace newer app attendance", {
                var patch: JSON = ["attendance": fixture(age: 30_000, open: false).object("attendance"),
                                   "updatedAt": ["lectures": HongsiClock.milliseconds(now) + 1_000],
                                   "errors": ["lectures": "old failure"], "pendingReceipts": [] as [JSON]]
                WidgetStore.discardOlderAttendance(current: fixture(age: 0), patch: &patch)
                try expect(patch["attendance"] == nil, "Older native response was accepted")
                try expect(patch.object("updatedAt")["lectures"] == nil && patch.object("errors")["lectures"] == nil, "Old metadata overwrote newer state")
                try expect(patch["pendingReceipts"] != nil, "Unrelated patch data was removed")
            }),
            ("new and equal attendance responses still apply", {
                for age in [0.0, 10_000] {
                    var patch: JSON = ["attendance": fixture(age: 0, open: false).object("attendance")]
                    WidgetStore.discardOlderAttendance(current: fixture(age: age), patch: &patch)
                    try expect(patch["attendance"] != nil, "New or equal response was discarded")
                }
            }),
        ]
        for (name, test) in tests {
            do { try test(); print("PASS: \(name)") }
            catch { throw Failure(description: "FAIL: \(name): \(error)") }
        }
        print("\(tests.count) iOS attendance regression tests passed")
    }
}
