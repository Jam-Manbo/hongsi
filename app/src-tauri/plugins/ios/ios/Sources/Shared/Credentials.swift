import Foundation
import Security

enum Credentials {
    private static func query() throws -> [String: Any] {
        guard let group = Bundle.main.object(forInfoDictionaryKey: "HongsiKeychainAccessGroup") as? String,
              !group.isEmpty, !group.contains("$(") else { throw WidgetFailure("자동 로그인 보안 설정을 확인해 주세요.") }
        return [kSecClass as String: kSecClassGenericPassword, kSecAttrService as String: "hongsi-app",
                kSecAttrAccount as String: "auto-login", kSecAttrAccessGroup as String: group]
    }
    private static func readUnlocked() throws -> String? {
        var query = try query(); query[kSecReturnData as String] = true; query[kSecMatchLimit as String] = kSecMatchLimitOne
        var result: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &result)
        if status == errSecItemNotFound { return nil }
        guard status == errSecSuccess, let data = result as? Data, let secret = String(data: data, encoding: .utf8) else {
            throw WidgetFailure("자동 로그인 정보를 읽지 못했어요.")
        }
        return secret
    }
    private static func writeUnlocked(_ secret: String?) throws {
        let query = try query()
        guard let secret else {
            let status = SecItemDelete(query as CFDictionary)
            guard status == errSecSuccess || status == errSecItemNotFound else { throw WidgetFailure("자동 로그인 정보를 삭제하지 못했어요.") }
            return
        }
        let values: [String: Any] = [kSecValueData as String: Data(secret.utf8),
            kSecAttrAccessible as String: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly]
        var status = SecItemUpdate(query as CFDictionary, values as CFDictionary)
        if status == errSecItemNotFound { status = SecItemAdd(query.merging(values) { _, new in new } as CFDictionary, nil) }
        guard status == errSecSuccess else { throw WidgetFailure("자동 로그인 정보를 저장하지 못했어요.") }
    }
    static func read() throws -> String? { try WidgetStore.locked("credentials") { try readUnlocked() } }
    static func write(_ secret: String?) throws { try WidgetStore.locked("credentials") { try writeUnlocked(secret) } }
    @discardableResult static func compareAndSet(expected: String, replacement: String?) throws -> Bool {
        try WidgetStore.locked("credentials") {
            guard try readUnlocked() == expected else { return false }
            try writeUnlocked(replacement); return true
        }
    }
}
