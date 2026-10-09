import Foundation
import Tauri
import UIKit

private struct AppStoreLookup: Decodable {
    let resultCount: Int
    let results: [AppStoreApp]
}

private struct AppStoreApp: Decodable {
    let bundleId: String?
    let trackId: Int?
    let version: String?
    let fileSizeBytes: String?
    let releaseNotes: String?
}

@MainActor
final class HongsiAppUpdate {
    private var storeURL: URL?
    private let session: URLSession = {
        let config = URLSessionConfiguration.ephemeral
        config.timeoutIntervalForRequest = 10
        config.timeoutIntervalForResource = 10
        config.requestCachePolicy = .reloadIgnoringLocalCacheData
        return URLSession(configuration: config)
    }()
    private var currentVersion: String { Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "" }

    func perform(_ action: String) async throws -> JsonObject {
        switch action {
        case "status": return status()
        case "check":
            storeURL = nil
            do { return try await check() }
            catch { throw WidgetFailure("업데이트 정보를 확인하지 못했어요.") }
        case "install":
            guard let url = storeURL else { throw WidgetFailure("업데이트를 다시 확인해 주세요.") }
            let opened = await UIApplication.shared.open(url, options: [:])
            guard opened else { throw WidgetFailure("App Store를 열지 못했어요.") }
            return ["state": "store-opened"]
        default: throw WidgetFailure("요청을 처리하지 못했어요.")
        }
    }

    private func status() -> JsonObject {
        ["source": "appstore", "currentVersion": currentVersion,
         "currentCode": Int(Bundle.main.object(forInfoDictionaryKey: "CFBundleVersion") as? String ?? "") ?? 0,
         "canInstall": true, "installError": NSNull()]
    }

    private func check() async throws -> JsonObject {
        guard let bundleId = Bundle.main.bundleIdentifier else { throw WidgetFailure("업데이트 정보를 확인하지 못했어요.") }
        var components = URLComponents(string: "https://itunes.apple.com/lookup")!
        components.queryItems = [URLQueryItem(name: "bundleId", value: bundleId), URLQueryItem(name: "country", value: "kr")]
        let (data, response) = try await session.data(from: components.url!)
        guard let response = response as? HTTPURLResponse, response.statusCode == 200 else { throw WidgetFailure("업데이트 정보를 확인하지 못했어요.") }
        let lookup = try JSONDecoder().decode(AppStoreLookup.self, from: data)
        var result = status()
        result["configured"] = lookup.resultCount > 0
        result["release"] = NSNull()
        if lookup.resultCount == 0 && lookup.results.isEmpty { return result }
        guard let app = lookup.results.first(where: { $0.bundleId == bundleId }),
              let version = app.version, let id = app.trackId, id > 0 else { throw WidgetFailure("업데이트 정보를 확인하지 못했어요.") }
        if try Self.isNewer(version, than: currentVersion) {
            storeURL = URL(string: "https://apps.apple.com/kr/app/id\(id)")
            result["release"] = ["version": version, "versionCode": 0,
                                 "size": app.fileSizeBytes.flatMap(Int.init) as Any? ?? NSNull(),
                                 "notes": app.releaseNotes ?? ""]
        }
        return result
    }

    static func isNewer(_ candidate: String, than installed: String) throws -> Bool {
        func parts(_ value: String) throws -> [Int] {
            let components = value.split(separator: ".", omittingEmptySubsequences: false)
            guard (1...3).contains(components.count), components.allSatisfy({ !$0.isEmpty && $0.allSatisfy({ $0.isASCII && $0.isNumber }) }) else {
                throw WidgetFailure("앱 버전을 확인하지 못했어요.")
            }
            let numbers = components.compactMap { Int($0) }
            guard numbers.count == components.count else { throw WidgetFailure("앱 버전을 확인하지 못했어요.") }
            return numbers + Array(repeating: 0, count: 3 - numbers.count)
        }
        let latest = try parts(candidate), current = try parts(installed)
        return current.lexicographicallyPrecedes(latest)
    }
}
