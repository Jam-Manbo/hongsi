import Foundation

extension HongsiProvider {
    static func preview(kind: String) -> HongsiEntry {
        func date(_ value: String) -> Date {
            let formatter = ISO8601DateFormatter()
            return formatter.date(from: value + "+09:00")!
        }
        func slot(_ name: String, _ day: Int, _ hour: Int, _ duration: Int, _ room: String, _ color: String) -> JSON {
            ["name": name, "code": name, "weekday": day, "start": String(format: "%02d:00", hour),
             "periods": Array((hour - 8)..<(hour - 8 + duration)), "room": room, "color": color]
        }
        let now = date(kind == "ATTENDANCE" ? "2026-03-09T10:05:00" : "2026-03-09T10:30:00")
        let slots: [JSON]
        if kind == "ATTENDANCE" {
            slots = [slot("대학수학(1)", 0, 10, 1, "C819", "#f59e0b")]
        } else if kind == "WEEK" {
            slots = [slot("대학수학(1)", 0, 10, 1, "C819", "#22c55e"),
                     slot("대학수학(1)", 3, 10, 2, "C819", "#22c55e"),
                     slot("교양일본어", 0, 12, 1, "C315", "#3b82f6"),
                     slot("교양일본어", 1, 12, 2, "C315", "#3b82f6"),
                     slot("논리적사고와글쓰기", 2, 11, 2, "C709", "#ef4444"),
                     slot("논리적사고와글쓰기", 4, 11, 2, "C709", "#ef4444"),
                     slot("전공기초영어", 3, 13, 2, "D0104", "#f59e0b"),
                     slot("컴퓨터공학개론", 0, 14, 2, "T0503", "#a855f7"),
                     slot("컴퓨터공학개론", 1, 15, 1, "T0503", "#a855f7")]
        } else {
            slots = [slot("대학수학(1)", 0, 10, 1, "C819", "#f59e0b"),
                     slot("교양일본어", 0, 12, 1, "C315", "#3b82f6"),
                     slot("전공기초영어", 0, 14, 1, "D0104", "#a855f7")]
        }
        func deadline(_ key: String, _ title: String, _ course: String, _ kind: String, _ status: String, _ color: String, _ due: String) -> JSON {
            ["key": key, "title": title, "course": course, "kind": kind, "status": status,
             "color": color, "due": date(due).timeIntervalSince1970]
        }
        let data: JSON = ["version": 1, "owner": String(repeating: "0", count: 64), "timetableLoaded": true, "slots": slots,
            "preferences": ["midnight": "prev", "semesterDisplay": "current", "timetableDisplay": "fit"], "semesterDisplay": "current",
            "attendance": AttendanceSnapshot.make(slots: slots, active: [["name": "대학수학(1)", "code": "대학수학(1)", "time": "월2", "key": "preview"]], courses: [], receipts: [], previous: [:], now: now),
            "deadlines": [deadline("preview-1", "실습1", "논리적사고와글쓰기", "assign", "미제출", "#ef4444", "2026-03-10T00:00:00"),
                          deadline("preview-2", "Video Quiz", "전공기초영어", "assign", "제출 완료", "#f59e0b", "2026-03-13T23:59:00"),
                          deadline("preview-3", "8장 단어 퀴즈", "교양일본어", "assign", "미제출", "#3b82f6", "2026-03-14T00:00:00"),
                          deadline("preview-4", "인간심리의이해 3-1", "인간심리의이해", "vod", "미시청", "#a855f7", "2026-03-24T23:59:00")],
            "seat": ["id": 1, "buildingName": "제4공학관", "building": "T", "roomName": "3층 제 1열람실", "seatNo": "20", "extendCount": 0, "expiresAt": now.addingTimeInterval(5400).timeIntervalSince1970]]
        return HongsiEntry(date: now, data: data, state: [:], theme: "system")
    }
}
