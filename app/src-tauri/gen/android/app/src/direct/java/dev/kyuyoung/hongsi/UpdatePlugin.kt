package dev.kyuyoung.hongsi

import android.app.Activity
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.PackageInfo
import android.content.pm.PackageInstaller
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.provider.Settings
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import org.json.JSONObject
import java.io.File
import java.net.HttpURLConnection
import java.net.URL
import java.util.concurrent.Executors

@InvokeArg
class UpdateArgs { var versionCode: Long = 0 }

@Suppress("DEPRECATION")
internal fun packageCode(info: PackageInfo): Long = if (Build.VERSION.SDK_INT >= 28) info.longVersionCode else info.versionCode.toLong()
@Suppress("DEPRECATION")
internal fun packageSigners(info: PackageInfo): Set<String> =
    (if (Build.VERSION.SDK_INT >= 28) info.signingInfo?.apkContentsSigners else info.signatures)?.map { it.toCharsString() }?.toSet() ?: emptySet()

@TauriPlugin
class UpdatePlugin(private val host: Activity) : Plugin(host) {
    private val executor = Executors.newSingleThreadExecutor()
    @Volatile private var candidate: AppRelease? = null
    private val prefs get() = host.getSharedPreferences("app-update", Context.MODE_PRIVATE)
    private val directory get() = File(host.cacheDir, "app-update").apply { mkdirs() }
    private val signatureFlags get() = if (Build.VERSION.SDK_INT >= 28) PackageManager.GET_SIGNING_CERTIFICATES else PackageManager.GET_SIGNATURES
    @Suppress("DEPRECATION")
    private fun installed() = host.packageManager.getPackageInfo(host.packageName, signatureFlags)
    private fun allowed() = Build.VERSION.SDK_INT < 26 || host.packageManager.canRequestPackageInstalls()
    private fun info(): JSObject = JSObject().apply {
        val current = installed()
        put("currentVersion", current.versionName ?: BuildConfig.VERSION_NAME)
        put("currentCode", packageCode(current))
        put("source", "direct")
        put("canInstall", allowed())
        put("installError", prefs.getString("error", null) ?: JSONObject.NULL)
    }
    private fun job(invoke: Invoke, action: () -> JSObject) {
        executor.execute {
            try { invoke.resolve(action()) }
            catch (e: Exception) { invoke.reject(e.message?.take(300) ?: "업데이트를 진행하지 못했어요.") }
        }
    }
    private fun connection(address: String): HttpURLConnection {
        var value = address
        repeat(6) {
            UpdatePolicy.https(value)
            val conn = URL(value).openConnection() as HttpURLConnection
            conn.instanceFollowRedirects = false
            conn.connectTimeout = 15000
            conn.readTimeout = 30000
            conn.setRequestProperty("User-Agent", "Hongsi-Android/${BuildConfig.VERSION_NAME}")
            conn.setRequestProperty("Cache-Control", "no-cache")
            if (conn.responseCode in listOf(301, 302, 303, 307, 308)) {
                val next = conn.getHeaderField("Location")
                conn.disconnect()
                require(next != null) { "업데이트 서버 주소를 확인하지 못했어요." }
                value = URL(URL(value), next).toString()
            } else return conn
        }
        error("업데이트 주소가 반복해서 변경되어 다운로드할 수 없어요.")
    }
    private fun latest(): AppRelease? {
        val conn = connection(BuildConfig.HONGSI_UPDATE_URL)
        try {
            if (conn.responseCode == 204) return null
            require(conn.responseCode == 200) { "업데이트 서버에 연결하지 못했어요." }
            val bytes = conn.inputStream.use { it.readBytesLimited(64 * 1024) }
            val json = JSONObject(bytes.toString(Charsets.UTF_8))
            return AppRelease(json.getString("version"), json.getLong("versionCode"), json.getString("url"), json.getString("sha256"), json.getLong("size"), json.optString("notes", ""))
        } finally { conn.disconnect() }
    }
    @Command fun status(invoke: Invoke) = job(invoke) { info() }
    @Command fun check(invoke: Invoke) = job(invoke) {
        candidate = null
        val release = latest()
        val result = info()
        result.put("configured", release != null)
        candidate = release?.takeIf { it.versionCode > packageCode(installed()) }
        result.put("release", candidate?.let { JSObject().apply {
            put("version", it.version); put("versionCode", it.versionCode); put("size", it.size); put("notes", it.notes)
        } } ?: JSONObject.NULL)
        result
    }
    @Command fun permissions(invoke: Invoke) {
        host.runOnUiThread {
            try {
                if (Build.VERSION.SDK_INT >= 26 && !allowed()) host.startActivity(Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, Uri.parse("package:${host.packageName}")))
                invoke.resolve(JSObject())
            } catch (_: Exception) { invoke.reject("설정에서 홍시의 ‘이 출처 허용’을 켜 주세요.") }
        }
    }
    @Command fun install(invoke: Invoke) {
        val requested = invoke.parseArgs(UpdateArgs::class.java).versionCode
        job(invoke) {
            val release = candidate ?: error("최신 버전을 다시 확인해 주세요.")
            require(requested == release.versionCode) { "업데이트 정보가 바뀌었어요. 다시 확인해 주세요." }
            if (!allowed()) return@job JSObject().apply { put("state", "permission-required") }
            prefs.edit().remove("error").apply()
            val file = File(directory, "hongsi-${release.versionCode}.apk")
            if (file.exists()) {
                try { UpdatePolicy.verifyFile(file, release) } catch (_: Exception) { file.delete() }
            }
            if (!file.exists()) download(release, file)
            UpdatePolicy.verifyFile(file, release)
            @Suppress("DEPRECATION")
            val archive = host.packageManager.getPackageArchiveInfo(file.path, signatureFlags) ?: error("APK 파일을 읽지 못했어요.")
            val current = installed()
            UpdatePolicy.verifyPackage(packageCode(current), release.versionCode, packageCode(archive), host.packageName, archive.packageName, packageSigners(current), packageSigners(archive))
            beginInstall(file, release)
            JSObject().apply { put("state", "installer-opened") }
        }
    }
    private fun download(release: AppRelease, file: File) {
        directory.listFiles()?.forEach { it.delete() }
        val partial = File(directory, "download.part")
        val conn = connection(release.url)
        try {
            require(conn.responseCode == 200) { "APK를 다운로드하지 못했어요." }
            require(conn.contentLengthLong <= UpdatePolicy.MAX_APK) { "업데이트 파일이 너무 커요." }
            val deadline = System.nanoTime() + 10L * 60 * 1000000000
            conn.inputStream.use { input -> partial.outputStream().use { output ->
                val buffer = ByteArray(64 * 1024); var received = 0L
                while (true) {
                    val count = input.read(buffer); if (count < 0) break
                    received += count
                    require(received <= release.size && System.nanoTime() < deadline) { "다운로드 용량이나 대기 시간이 제한을 초과했어요." }
                    output.write(buffer, 0, count)
                }
            } }
            UpdatePolicy.verifyFile(partial, release)
            require(partial.renameTo(file)) { "업데이트 파일을 저장하지 못했어요." }
        } finally { conn.disconnect(); partial.delete() }
    }
    private fun beginInstall(file: File, release: AppRelease) {
        val installer = host.packageManager.packageInstaller
        val params = PackageInstaller.SessionParams(PackageInstaller.SessionParams.MODE_FULL_INSTALL).apply {
            setAppPackageName(host.packageName); setSize(release.size)
            if (Build.VERSION.SDK_INT >= 31) setRequireUserAction(PackageInstaller.SessionParams.USER_ACTION_REQUIRED)
        }
        val id = installer.createSession(params)
        try {
            installer.openSession(id).use { session ->
                session.openWrite("base.apk", 0, release.size).use { output ->
                    file.inputStream().use { it.copyTo(output) }; session.fsync(output)
                }
                val flags = PendingIntent.FLAG_UPDATE_CURRENT or (if (Build.VERSION.SDK_INT >= 31) PendingIntent.FLAG_MUTABLE else 0)
                val intent = Intent(host, UpdateInstallReceiver::class.java).setAction("${host.packageName}.UPDATE_RESULT")
                session.commit(PendingIntent.getBroadcast(host, id, intent, flags).intentSender)
            }
        } catch (e: Exception) { installer.abandonSession(id); throw e }
    }
}

private fun java.io.InputStream.readBytesLimited(limit: Int): ByteArray {
    val output = java.io.ByteArrayOutputStream(); val buffer = ByteArray(8192)
    while (true) { val count = read(buffer); if (count < 0) break; require(output.size() + count <= limit) { "업데이트 정보가 너무 커요." }; output.write(buffer, 0, count) }
    return output.toByteArray()
}

class UpdateInstallReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val prefs = context.getSharedPreferences("app-update", Context.MODE_PRIVATE)
        when (intent.getIntExtra(PackageInstaller.EXTRA_STATUS, PackageInstaller.STATUS_FAILURE)) {
            PackageInstaller.STATUS_PENDING_USER_ACTION -> {
                @Suppress("DEPRECATION")
                val confirmation = intent.getParcelableExtra<Intent>(Intent.EXTRA_INTENT)
                try {
                    require(confirmation != null)
                    context.startActivity(confirmation.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
                } catch (_: Exception) { prefs.edit().putString("error", "설치 확인 화면을 열지 못했어요. 업데이트 버튼으로 다시 시도해 주세요.").apply() }
            }
            PackageInstaller.STATUS_SUCCESS -> prefs.edit().remove("error").apply()
            PackageInstaller.STATUS_FAILURE_ABORTED -> prefs.edit().putString("error", "업데이트 설치를 취소했어요.").apply()
            else -> prefs.edit().putString("error", "Android에서 업데이트를 설치하지 못했어요. 저장 공간과 설치 권한을 확인해 주세요.").apply()
        }
    }
}
