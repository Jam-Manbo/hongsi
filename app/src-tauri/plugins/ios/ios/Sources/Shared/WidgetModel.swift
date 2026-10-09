import Foundation

enum AttendanceSnapshot {
    static let labels = ["present": "출석", "late": "지각", "excused": "공결", "absent": "결석"]
    static func sameCourse(_ a: JSON, _ b: JSON) -> Bool {
        if !a.text("code").isEmpty && !b.text("code").isEmpty { return a.text("code") == b.text("code") }
        return normalized(a.text("name")) == normalized(b.text("name"))
    }
    static func normalized(_ name: String) -> String { name.replacingOccurrences(of: "\\(\\*\\)|\\s", with: "", options: .regularExpression) }
    static func valid(_ receipt: JSON, now: Date = Date()) -> Bool {
        receipt.text("date") == HongsiClock.text(now) && ["present", "late", "excused"].contains(receipt.text("kind")) &&
        !receipt.object("lecture").text("key").isEmpty && HongsiClock.text(Date(timeIntervalSince1970: receipt.number("confirmedAt") / 1000)) == HongsiClock.text(now)
    }
    static func identity(_ item: JSON, now: Date) -> String {
        let course = item.text("code").isEmpty ? normalized(item.text("name")) : item.text("code")
        let session = item["at"] != nil ? String(Int64(item.number("at"))) : item.text("key", "course")
        return "\(HongsiClock.text(now))|\(course)|\(session)"
    }
    static func clock(_ value: String) -> String? {
        guard let range = value.range(of: "[0-9]{1,2}:[0-9]{2}", options: .regularExpression) else { return nil }
        return HongsiClock.hm(HongsiClock.minutes(String(value[range])))
    }
    static func schedule(_ item: JSON, sessions: [JSON] = [], now: Date = Date()) -> String {
        if !item.text("scheduleLabel").isEmpty { return item.text("scheduleLabel") }
        let matching = sessions.filter { sameCourse($0, item) }
        let time = clock(item.text("time"))
        let period = item.text("time").replacingOccurrences(of: "교시", with: "").trimmingCharacters(in: .whitespaces)
        let periodDay = period.first.flatMap { "월화수목금토일".firstIndex(of: $0) }.map { "월화수목금토일".distance(from: "월화수목금토일".startIndex, to: $0) }
        let periodNumber = Int(period.dropFirst().trimmingCharacters(in: .whitespaces))
        let candidates = matching.filter {
            if let time { return $0.text("start") == time }
            if let periodDay, let periodNumber { return $0.integer("weekday") == periodDay && ($0["periods"] as? [Int])?.first == periodNumber }
            return (HongsiClock.milliseconds(now) - $0.number("at")) >= -180_000 && (HongsiClock.milliseconds(now) - $0.number("at")) <= 600_000
        }
        let slot: JSON? = item["weekday"] != nil ? item : candidates.count == 1 ? candidates[0] : time == nil && periodDay == nil && matching.count == 1 ? matching[0] : nil
        guard let slot else { return item.text("time") }
        let days = Array("월화수목금토일"); let day = slot.integer("weekday", -1)
        let periods = slot["periods"] as? [Int] ?? []
        let number = periods.first.map(String.init) ?? ""
        return [(days.indices.contains(day) ? String(days[day]) : "") + number,
                slot.text("start").isEmpty ? "" : slot.text("start") + " 수업", slot.text("room")].filter { !$0.isEmpty }.joined(separator: " ")
    }
    static func sessions(slots: [JSON], now: Date) -> [JSON] {
        let midnight = HongsiClock.calendar.startOfDay(for: now).timeIntervalSince1970 * 1000
        return slots.filter { $0.integer("weekday") == HongsiClock.weekday(now) }.flatMap { slot -> [JSON] in
            let periods = slot["periods"] as? [Int] ?? [1]
            return periods.enumerated().map { index, period in
                var value = slot; let start = HongsiClock.minutes(slot.text("start")) + (period - (periods.first ?? period)) * 60
                value["start"] = HongsiClock.hm(start); value["at"] = midnight + Double(start) * 60_000
                value["round"] = index + 1; value["periods"] = [period]; return value
            }
        }.sorted { $0.number("at") < $1.number("at") }
    }
    static func make(slots: [JSON], active: [JSON], courses: [JSON], receipts: [JSON], previous: JSON, now: Date = Date()) -> JSON {
        let today = HongsiClock.text(now), ms = HongsiClock.milliseconds(now)
        let sessions = sessions(slots: slots, now: now)
        func reference(_ lecture: JSON, at: Double) -> JSON {
            let time = clock(lecture.text("time"))
            let matches = sessions.filter { sameCourse($0, lecture) && (time != nil ? $0.text("start") == time! : at >= $0.number("at") - 180_000 && at <= $0.number("at") + 600_000) }
            if matches.count == 1 { var item = matches[0]; item["key"] = lecture.text("key"); return item }
            return lecture
        }
        func mark(_ item: JSON) -> JSON? {
            let matching = courses.filter { sameCourse($0, item) }
            if matching.count == 1, matching[0].flag("published"), item["at"] != nil {
                let lessonSessions = sessions.filter { sameCourse($0, item) }
                let entries = matching[0].objects("weeks").flatMap { $0.objects("sessions") }.filter { $0.text("date") == HongsiClock.text(now, "M/d") }
                if entries.count == lessonSessions.count, let index = lessonSessions.firstIndex(where: { $0.number("at") == item.number("at") }) {
                    let entry = entries[index]; let label = labels[entry.text("kind")] ?? (entry.text("kind") == "other" ? entry.text("mark") : "")
                    if !label.isEmpty { return ["kind": entry.text("kind"), "label": label, "source": "school"] }
                }
            }
            let receipt = receipts.filter { valid($0, now: now) }.sorted { $0.number("confirmedAt") > $1.number("confirmedAt") }.first {
                let lecture = $0.object("lecture")
                return (!item.text("key").isEmpty && lecture.text("key") == item.text("key")) || identity(reference(lecture, at: $0.number("confirmedAt")), now: now) == identity(item, now: now)
            }
            return receipt.map { ["kind": $0.text("kind"), "label": labels[$0.text("kind")] ?? "", "source": "app"] }
        }
        let opened = active.map { lecture -> JSON in
            let item = reference(lecture, at: ms); var value = lecture
            value["scheduleLabel"] = schedule(lecture, sessions: sessions, now: now)
            value["identity"] = identity(item, now: now); value["mark"] = mark(item) as Any? ?? NSNull(); return value
        }
        let seen = Set(previous.objects("sessions").filter { $0.flag("seenOpen") }.map { $0.text("identity") } + opened.map { $0.text("identity") })
        return ["date": today, "loaded": true, "error": "", "timetableLoaded": true, "timetableError": "", "checkedAt": ms,
                "sessions": sessions.map { item -> JSON in
                    var value = item; value["identity"] = identity(item, now: now); value["mark"] = mark(item) as Any? ?? NSNull()
                    value["seenOpen"] = seen.contains(identity(item, now: now)); return value
                }, "active": opened]
    }
}

struct AttendanceStatus {
    var title = ""
    var message = ""
    var detail = ""
    var available = false
    var tone = "muted"
    var identity = ""
    var kind = ""
}

enum WidgetModel {
    // Match the app and Android display cache; submission still rechecks with the school.
    static let attendanceMaxAge: TimeInterval = 10 * 60
    private static let attended = Set(["present", "late", "excused"])

    static func attendanceReloadDate(_ data: JSON, now: Date) -> Date {
        let normal = now.addingTimeInterval(15 * 60)
        guard !data.text("owner").isEmpty else { return normal }
        let snapshot = data.object("attendance"), ms = HongsiClock.milliseconds(now)
        let sameDay = snapshot.text("date") == HongsiClock.text(now)
        let age = ms - snapshot.number("checkedAt")
        let soon = now.addingTimeInterval(5 * 60)
        if sameDay && age >= 0 && age < attendanceMaxAge * 1000 && snapshot.objects("active").contains(where: {
            !attended.contains($0.object("mark").text("kind"))
        }) { return soon }
        let saved = sameDay ? snapshot.objects("sessions") : []
        let unfinished = AttendanceSnapshot.sessions(slots: data.objects("slots"), now: now).filter { session in
            let mark = saved.first { AttendanceSnapshot.sameCourse($0, session) && $0.number("at") == session.number("at") }?.object("mark") ?? [:]
            return !attended.contains(mark.text("kind"))
        }
        if unfinished.contains(where: { ms >= $0.number("at") - 180_000 && ms < $0.number("at") + 3_600_000 }) { return soon }
        if let next = unfinished.first(where: { $0.number("at") - 180_000 > ms }) {
            let watchStart = Date(timeIntervalSince1970: next.number("at") / 1000 - 180)
            return min(normal, max(soon, watchStart))
        }
        return normal
    }

    static func attendanceTimelineDates(_ data: JSON, state: JSON, now: Date) -> [Date] {
        var dates = [now]
        for step in 1...12 { dates.append(now.addingTimeInterval(Double(step * 5 * 60))) }
        let snapshot = data.object("attendance")
        if snapshot.text("date") == HongsiClock.text(now) && !snapshot.objects("active").isEmpty {
            let expiry = Date(timeIntervalSince1970: snapshot.number("checkedAt") / 1000 + attendanceMaxAge)
            if expiry > now { dates.append(expiry) }
        }
        for (key, duration) in [("inputAt", 600.0), ("pendingAt", 45.0)] {
            let expiry = Date(timeIntervalSince1970: state.number(key) / 1000 + duration)
            if expiry > now { dates.append(expiry) }
        }
        return Array(Set(dates)).sorted()
    }
    static func hasTimetable(_ data: JSON) -> Bool { data.flag("timetableLoaded") || data.object("updatedAt").number("timetable") > 0 || !data.objects("slots").isEmpty }
    static func today(_ data: JSON, now: Date) -> [JSON] {
        data.objects("slots").filter { $0.integer("weekday") == HongsiClock.weekday(now) }.sorted { $0.text("start") < $1.text("start") }
    }
    static func deadlineDay(_ item: JSON, data: JSON) -> Int {
        let due = item.number("due"), previous = data.object("preferences").text("midnight", "prev") != "same" && HongsiClock.text(Date(timeIntervalSince1970: due), "HH:mm") == "00:00"
        return Int(floor((due + 32400 - (previous ? 60 : 0)) / 86400))
    }
    static func deadlineLabel(_ item: JSON, data: JSON) -> String {
        if item.isNull("due") { return "마감 없음" }
        let due = Date(timeIntervalSince1970: item.number("due"))
        let previous = data.object("preferences").text("midnight", "prev") != "same" && HongsiClock.text(due, "HH:mm") == "00:00"
        let date = previous ? due.addingTimeInterval(-60) : due
        return HongsiClock.text(date, "M/d(E)") + (item.flag("allDay") ? " 하루 종일" : (previous ? " 24:00" : " " + HongsiClock.text(due, "HH:mm")) + " 마감")
    }
    static func deadlines(_ data: JSON, now: Date) -> [JSON] {
        guard data.text("semesterDisplay") == data.object("preferences").text("semesterDisplay", "current") else { return [] }
        return data.objects("deadlines").filter { !$0.flag("done") && !$0.isNull("due") && $0.number("due") > now.timeIntervalSince1970 }.sorted { a, b in
            let ad = deadlineDay(a, data: data), bd = deadlineDay(b, data: data)
            if ad != bd { return ad < bd }
            if a.flag("allDay") != b.flag("allDay") { return !a.flag("allDay") }
            if a.number("due") != b.number("due") { return a.number("due") < b.number("due") }
            let compare = a.text("title").compare(b.text("title"), options: [.numeric], locale: Locale(identifier: "ko_KR"))
            return compare == .orderedSame ? a.text("key") < b.text("key") : compare == .orderedAscending
        }
    }
    static func attendance(_ data: JSON, state: JSON, now: Date) -> AttendanceStatus {
        if data.isEmpty { return AttendanceStatus(message: "로그인이 필요해요.", detail: "앱에서 로그인해 주세요.") }
        let snapshot = data.object("attendance"), ms = HongsiClock.milliseconds(now)
        if snapshot.isEmpty { return AttendanceStatus(message: "출석 정보를 확인해 주세요.", detail: "앱을 한 번 열어 주세요.") }
        let sameDay = snapshot.text("date") == HongsiClock.text(now)
        let age = ms - snapshot.number("checkedAt")
        let sessions = sameDay ? snapshot.objects("sessions") : []
        let current = sessions.first { ms >= $0.number("at") - 180_000 && ms <= $0.number("at") + 600_000 }
        let fresh = snapshot.text("error").isEmpty && sameDay && age >= 0 && age <= 20_000
        let active = sameDay && age >= 0 && age < attendanceMaxAge * 1000 ? snapshot.objects("active") : []
        func success(_ value: AttendanceStatus) -> AttendanceStatus {
            let receipt = state.object("localReceipt")
            guard receipt.text("identity") == value.identity, !value.identity.isEmpty, receipt.text("date") == HongsiClock.text(now) else { return value }
            var result = value; result.kind = receipt.text("kind"); result.available = false
            result.message = result.kind == "late" ? "지각 처리됨" : result.kind == "excused" ? "공결 처리됨" : "출석 완료"
            result.tone = result.kind == "late" ? "warn" : "success"; return result
        }
        func marked(_ item: JSON, _ mark: JSON) -> AttendanceStatus {
            let kind = mark.text("kind")
            return AttendanceStatus(title: item.text("name"), message: kind == "present" ? "출석 완료" : mark.text("label") + " 처리됨",
                detail: AttendanceSnapshot.schedule(item, sessions: sessions, now: now), tone: ["present", "excused"].contains(kind) ? "success" : kind == "absent" ? "error" : kind == "late" ? "warn" : "muted", identity: item.text("identity"), kind: kind)
        }
        if let opened = active.first(where: { !attended.contains($0.object("mark").text("kind")) }) {
            return success(AttendanceStatus(title: opened.text("name"), message: "출석할 수 있어요.", detail: AttendanceSnapshot.schedule(opened, sessions: sessions, now: now), available: true, tone: "primary", identity: opened.text("identity"), kind: "available"))
        }
        if let current {
            if !current.object("mark").isEmpty { return marked(current, current.object("mark")) }
            var status = success(AttendanceStatus(title: current.text("name"), message: "출석 가능 여부 확인 중", detail: AttendanceSnapshot.schedule(current, now: now), tone: "primary", identity: current.text("identity"), kind: "waiting"))
            if status.kind != "waiting" { return status }
            if !snapshot.text("error").isEmpty { status.message = "출석 정보를 불러오지 못했어요."; status.tone = "error"; status.kind = "error" }
            else if !fresh { status.message = "출석 정보를 확인해 주세요."; status.tone = "warn"; status.kind = "stale" }
            else if current.flag("seenOpen") || ms >= current.number("at") { status.message = "출석 확인 불가"; status.tone = "warn"; status.kind = "unknown" }
            return status
        }
        if let item = active.first, !item.object("mark").isEmpty { return marked(item, item.object("mark")) }
        if !snapshot.text("error").isEmpty || !snapshot.text("timetableError").isEmpty { return AttendanceStatus(message: "출석 정보를 불러오지 못했어요.", tone: "error", kind: "error") }
        if !snapshot.flag("timetableLoaded") { return AttendanceStatus(message: "시간표를 확인해 주세요.", detail: "앱에서 시간표를 한 번 불러와 주세요.") }
        if snapshot.text("date") != HongsiClock.text(now) { return AttendanceStatus(message: "출석 정보를 확인해 주세요.", detail: "새로고침을 눌러 주세요.") }
        if sessions.isEmpty { return AttendanceStatus(message: "오늘은 수업이 없어요.") }
        if let next = sessions.first(where: { $0.number("at") > ms }) { return AttendanceStatus(title: next.text("name"), message: "다음 수업", detail: AttendanceSnapshot.schedule(next, now: now), kind: "next") }
        if let item = sessions.last(where: { ms >= $0.number("at") && ms < $0.number("at") + 3_600_000 }) {
            if !item.object("mark").isEmpty { return marked(item, item.object("mark")) }
            return success(AttendanceStatus(title: item.text("name"), message: "출석 확인 불가", detail: AttendanceSnapshot.schedule(item, now: now), tone: "warn", identity: item.text("identity"), kind: "unknown"))
        }
        return AttendanceStatus(message: "오늘 수업이 모두 끝났어요.")
    }
}
