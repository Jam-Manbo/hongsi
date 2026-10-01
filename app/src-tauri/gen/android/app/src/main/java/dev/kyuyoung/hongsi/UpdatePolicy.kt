package dev.kyuyoung.hongsi

import java.io.File
import java.net.URI
import java.security.MessageDigest

internal data class AppRelease(val version: String, val versionCode: Long, val url: String, val sha256: String, val size: Long, val notes: String) {
    init {
        require(version.matches(Regex("[0-9]+\\.[0-9]+\\.[0-9]+")) && versionCode > 0) { "업데이트 버전 정보가 올바르지 않아요." }
        UpdatePolicy.https(url)
        require(sha256.matches(Regex("[a-fA-F0-9]{64}")) && size in 1..UpdatePolicy.MAX_APK) { "업데이트 파일 정보가 올바르지 않아요." }
        require(notes.length <= 10000) { "업데이트 설명이 너무 길어요." }
    }
}

internal object UpdatePolicy {
    const val MAX_APK = 256L * 1024 * 1024
    fun https(value: String): URI {
        val uri = URI(value)
        require(uri.scheme == "https" && !uri.host.isNullOrBlank() && uri.userInfo == null && uri.fragment == null) { "업데이트는 안전한 HTTPS 주소에서만 받을 수 있어요." }
        return uri
    }
    fun sha256(file: File): String {
        val digest = MessageDigest.getInstance("SHA-256")
        file.inputStream().use { input ->
            val buffer = ByteArray(64 * 1024)
            while (true) { val count = input.read(buffer); if (count < 0) break; digest.update(buffer, 0, count) }
        }
        return digest.digest().joinToString("") { "%02x".format(it) }
    }
    fun verifyFile(file: File, release: AppRelease) {
        require(file.length() == release.size && sha256(file).equals(release.sha256, true)) { "업데이트 파일 검증에 실패했어요. 다시 다운로드해 주세요." }
    }
    fun verifyPackage(installedCode: Long, expectedCode: Long, archiveCode: Long, expectedPackage: String, archivePackage: String, installedSigners: Set<String>, archiveSigners: Set<String>) {
        require(expectedCode > installedCode && archiveCode == expectedCode) { "APK 버전이 맞지 않습니다." }
        require(expectedPackage == archivePackage) { "홍시 앱의 업데이트 파일이 아니에요." }
        require(installedSigners.isNotEmpty() && installedSigners == archiveSigners) { "설치된 홍시와 서명키가 달라 업데이트할 수 없어요." }
    }
}
