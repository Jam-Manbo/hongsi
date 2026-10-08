import Foundation
import CryptoKit
import Darwin

typealias JSON = [String: Any]

extension Dictionary where Key == String, Value == Any {
    func text(_ key: String, _ fallback: String = "") -> String { self[key] as? String ?? fallback }
    func number(_ key: String, _ fallback: Double = 0) -> Double { (self[key] as? NSNumber)?.doubleValue ?? fallback }
    func integer(_ key: String, _ fallback: Int = 0) -> Int { (self[key] as? NSNumber)?.intValue ?? fallback }
    func flag(_ key: String) -> Bool { self[key] as? Bool ?? false }
    func object(_ key: String) -> JSON { self[key] as? JSON ?? [:] }
    func objects(_ key: String) -> [JSON] { self[key] as? [JSON] ?? [] }
    func isNull(_ key: String) -> Bool { self[key] == nil || self[key] is NSNull }
}

struct WidgetFailure: LocalizedError {
    let message: String
    var errorDescription: String? { message }
    init(_ message: String) { self.message = message }
}

enum HongsiClock {
    static let zone = TimeZone(identifier: "Asia/Seoul")!
    static var calendar: Calendar { var c = Calendar(identifier: .gregorian); c.timeZone = zone; return c }
    static func text(_ date: Date = Date(), _ format: String = "yyyy-MM-dd") -> String {
        let f = DateFormatter(); f.locale = Locale(identifier: "ko_KR"); f.timeZone = zone; f.dateFormat = format
        return f.string(from: date)
    }
    static func weekday(_ date: Date) -> Int { (calendar.component(.weekday, from: date) + 5) % 7 }
    static func minute(_ date: Date) -> Int { calendar.component(.hour, from: date) * 60 + calendar.component(.minute, from: date) }
    static func minutes(_ text: String) -> Int {
        let p = text.split(separator: ":").map { Int($0) ?? 0 }; return (p.first ?? 0) * 60 + (p.count > 1 ? p[1] : 0)
    }
    static func hm(_ minute: Int) -> String { String(format: "%02d:%02d", minute / 60, minute % 60) }
    static func milliseconds(_ date: Date = Date()) -> Double { date.timeIntervalSince1970 * 1000 }
}

enum WidgetStore {
    static let group = "group.dev.kyuyoung.hongsi"
    static func owner(_ account: String) -> String { SHA256.hash(data: Data(account.utf8)).map { String(format: "%02x", $0) }.joined() }
    static func directory() throws -> URL {
        guard let url = FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: group) else {
            throw WidgetFailure("앱과 위젯의 공유 저장소를 열지 못했어요.")
        }
        return url
    }
    static func locked<T>(_ name: String = "data", _ block: () throws -> T) throws -> T {
        let url = try directory().appendingPathComponent("\(name).lock")
        let fd = Darwin.open(url.path, O_CREAT | O_RDWR, S_IRUSR | S_IWUSR)
        guard fd >= 0 else { throw WidgetFailure("위젯 저장소를 열지 못했어요.") }
        defer { Darwin.close(fd) }
        guard flock(fd, LOCK_EX) == 0 else { throw WidgetFailure("위젯 저장소를 열지 못했어요.") }
        defer { flock(fd, LOCK_UN) }
        return try block()
    }
    static func readUnlocked(_ name: String) throws -> JSON {
        let url = try directory().appendingPathComponent(name + ".json")
        guard FileManager.default.fileExists(atPath: url.path) else { return [:] }
        let bytes = try Data(contentsOf: url)
        guard bytes.count <= 4_000_000, let data = try JSONSerialization.jsonObject(with: bytes) as? JSON else {
            throw WidgetFailure("위젯 데이터를 읽지 못했어요.")
        }
        return data
    }
    static func writeUnlocked(_ value: JSON, _ name: String) throws {
        let url = try directory().appendingPathComponent(name + ".json")
        let bytes = try JSONSerialization.data(withJSONObject: value, options: [.sortedKeys])
        try bytes.write(to: url, options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication])
        var resource = URLResourceValues(); resource.isExcludedFromBackup = true
        var mutable = url; try mutable.setResourceValues(resource)
    }
    static func read() -> JSON { (try? locked { try readUnlocked("widgets") }) ?? [:] }
    static func replace(_ raw: String) throws {
        guard raw.utf8.count <= 2_000_000 else { throw WidgetFailure("위젯 데이터가 너무 커요.") }
        var next: JSON = [:]
        if !raw.isEmpty {
            guard let data = raw.data(using: .utf8), let value = try JSONSerialization.jsonObject(with: data) as? JSON,
                  value.integer("version") == 1, !value.text("owner").isEmpty else { throw WidgetFailure("위젯 데이터를 읽지 못했어요.") }
            next = value; next["owner"] = owner(value.text("owner"))
        }
        try locked {
            let previous = try readUnlocked("widgets")
            if next.text("owner") != previous.text("owner") || next.isEmpty {
                try writeUnlocked([:], "state")
                try writeUnlocked([:], "navigation")
            } else {
                var times = next.object("updatedAt")
                let oldTimes = previous.object("updatedAt")
                if oldTimes.number("preferences") > times.number("preferences") {
                    var preferences = next.object("preferences")
                    for key in ["timetableDisplay", "semesterDisplay"] { preferences[key] = previous.object("preferences")[key] }
                    next["preferences"] = preferences; times["preferences"] = oldTimes["preferences"]
                }
                let display = next.object("preferences").text("semesterDisplay", "current")
                for (field, resource) in [("slots", "timetable"), ("deadlines", "calendar"), ("seat", "seats"), ("attendance", "lectures")] {
                    let matching = field != "deadlines" || previous.text("semesterDisplay") == display
                    if matching && (oldTimes.number(resource) > times.number(resource) || (field == "deadlines" && next.text("semesterDisplay") != display)) {
                        next[field] = previous[field]; times[resource] = oldTimes[resource]
                        if field == "slots" { next["timetableLoaded"] = previous.flag("timetableLoaded") }
                        if field == "deadlines" { next["semesterDisplay"] = previous["semesterDisplay"] }
                    }
                }
                next["updatedAt"] = times
                next["pendingReceipts"] = previous.objects("pendingReceipts")
            }
            try writeUnlocked(next, "widgets")
        }
    }
    @discardableResult static func merge(owner: String, patch: JSON) throws -> Bool {
        try locked {
            var data = try readUnlocked("widgets")
            guard !owner.isEmpty, data.text("owner") == owner else { return false }
            var patch = patch
            if patch.object("updatedAt").number("preferences") < data.object("updatedAt").number("preferences") {
                patch.removeValue(forKey: "preferences")
                var times = patch.object("updatedAt"); times.removeValue(forKey: "preferences"); patch["updatedAt"] = times
            }
            let display = patch.object("preferences").text("semesterDisplay", data.object("preferences").text("semesterDisplay", "current"))
            if patch["deadlines"] != nil && patch.text("semesterDisplay") != display { return false }
            for (key, value) in patch {
                if ["updatedAt", "errors", "preferences"].contains(key), let changes = value as? JSON {
                    var values = data.object(key); changes.forEach { values[$0.key] = $0.value }; data[key] = values
                } else { data[key] = value }
            }
            try writeUnlocked(data, "widgets"); return true
        }
    }
    static func state() -> JSON { (try? locked { try readUnlocked("state") }) ?? [:] }
    static func changeState(owner: String, _ change: (inout JSON) throws -> Void) throws {
        try locked {
            guard !owner.isEmpty, try readUnlocked("widgets").text("owner") == owner else { throw WidgetFailure("위젯을 새로고침해 주세요.") }
            var state = try readUnlocked("state"); try change(&state); try writeUnlocked(state, "state")
        }
    }
    static var theme: String { (try? locked { try readUnlocked("theme").text("value", "system") }) ?? "system" }
    static func setTheme(_ value: String) throws {
        guard ["system", "light", "dark"].contains(value) else { return }
        try locked { try writeUnlocked(["value": value], "theme") }
    }
    static func markChanged(owner: String) throws {
        try locked {
            guard try readUnlocked("widgets").text("owner") == owner else { return }
            var navigation = try readUnlocked("navigation"); navigation["changed"] = true
            try writeUnlocked(navigation, "navigation")
        }
    }
    static func url(kind: String, detail: String = "", owner: String) -> URL {
        var parts = URLComponents(); parts.scheme = "hongsi-widget"; parts.host = "open"
        parts.queryItems = [.init(name: "kind", value: kind), .init(name: "detail", value: detail), .init(name: "owner", value: owner)]
        return parts.url!
    }
    static func accept(_ url: URL) throws {
        guard url.scheme == "hongsi-widget", url.host == "open", url.absoluteString.count < 4096,
              let query = URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems else { return }
        func value(_ key: String) -> String { query.first { $0.name == key }?.value ?? "" }
        let kind = value("kind"), owner = value("owner")
        guard ["ATTENDANCE", "TODAY", "TODAY_LARGE", "DEADLINES", "DEADLINES_LARGE", "WEEK", "SEAT"].contains(kind), owner.count == 64 else { return }
        try locked {
            var navigation = try readUnlocked("navigation")
            navigation["target"] = ["kind": kind, "detail": value("detail"), "owner": owner]
            try writeUnlocked(navigation, "navigation")
        }
    }
    static func takeIntent() throws -> JSON {
        try locked {
            let data = try readUnlocked("widgets"), navigation = try readUnlocked("navigation")
            try writeUnlocked([:], "navigation")
            return ["owner": data.text("owner"), "receipts": data.objects("pendingReceipts").filter { AttendanceSnapshot.valid($0) },
                    "changed": navigation.flag("changed"), "target": navigation["target"] ?? NSNull()]
        }
    }
}
