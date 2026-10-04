package dev.kyuyoung.hongsi

import android.app.Activity
import android.os.Build
import android.webkit.WebView
import androidx.appcompat.app.AppCompatActivity
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import com.google.android.play.core.appupdate.AppUpdateInfo
import com.google.android.play.core.appupdate.AppUpdateManagerFactory
import com.google.android.play.core.appupdate.AppUpdateOptions
import com.google.android.play.core.install.model.AppUpdateType
import com.google.android.play.core.install.model.UpdateAvailability
import org.json.JSONObject
import java.net.HttpURLConnection
import java.net.URL
import java.util.concurrent.Executors

@InvokeArg
class UpdateArgs { var versionCode: Long = 0 }

@TauriPlugin
class UpdatePlugin(private var host: Activity) : Plugin(host) {
    private val manager = AppUpdateManagerFactory.create(host.applicationContext)
    private val executor = Executors.newSingleThreadExecutor()
    private var flowActive = false
    private var resuming = false
    private var lastError: String? = null

    @Suppress("DEPRECATION")
    private fun info(): JSObject {
        val current = host.packageManager.getPackageInfo(host.packageName, 0)
        return JSObject().apply {
            put("currentVersion", current.versionName ?: BuildConfig.VERSION_NAME)
            put("currentCode", if (Build.VERSION.SDK_INT >= 28) current.longVersionCode else current.versionCode.toLong())
            put("source", "play")
            put("canInstall", true)
            put("installError", lastError ?: JSONObject.NULL)
        }
    }

    @Command fun status(invoke: Invoke) {
        host.runOnUiThread { invoke.resolve(info()) }
    }

    private fun latestStable(): JSONObject? {
        val url = URL(BuildConfig.HONGSI_UPDATE_URL)
        require(url.protocol == "https") { "업데이트 주소가 올바르지 않아요." }
        val connection = (url.openConnection() as HttpURLConnection).apply {
            instanceFollowRedirects = false
            connectTimeout = 15000
            readTimeout = 15000
            setRequestProperty("Cache-Control", "no-cache")
        }
        try {
            if (connection.responseCode == 204) return null
            require(connection.responseCode == 200) { "업데이트 서버에 연결하지 못했어요." }
            val bytes = connection.inputStream.use { input ->
                val output = java.io.ByteArrayOutputStream()
                val buffer = ByteArray(8192)
                while (true) {
                    val count = input.read(buffer)
                    if (count < 0) break
                    require(output.size() + count <= 64 * 1024) { "업데이트 정보가 올바르지 않아요." }
                    output.write(buffer, 0, count)
                }
                output.toByteArray()
            }
            val release = JSONObject(bytes.toString(Charsets.UTF_8))
            require(!release.optBoolean("prerelease", false)
                && release.getString("version").matches(Regex("[0-9]+\\.[0-9]+\\.[0-9]+"))
                && release.getLong("versionCode") in 1..2100000000L) { "업데이트 정보가 올바르지 않아요." }
            return release
        } finally { connection.disconnect() }
    }

    private fun stable(action: (JSONObject?) -> Unit, failure: () -> Unit) {
        executor.execute {
            try {
                val release = latestStable()
                host.runOnUiThread { action(release) }
            } catch (_: Exception) { host.runOnUiThread { failure() } }
        }
    }

    @Command fun check(invoke: Invoke) {
        stable({ release ->
            manager.appUpdateInfo.addOnSuccessListener { update ->
                val available = (update.updateAvailability() == UpdateAvailability.UPDATE_AVAILABLE ||
                    update.updateAvailability() == UpdateAvailability.DEVELOPER_TRIGGERED_UPDATE_IN_PROGRESS) &&
                    release?.getLong("versionCode") == update.availableVersionCode().toLong()
                lastError = null
                val result = info().apply {
                    put("configured", release != null)
                    put("release", if (available) JSObject().apply {
                        put("version", release.getString("version"))
                        put("versionCode", update.availableVersionCode())
                        put("size", update.totalBytesToDownload().takeIf { it > 0 } ?: JSONObject.NULL)
                        put("notes", release.optString("notes", ""))
                    } else JSONObject.NULL)
                }
                invoke.resolve(result)
            }.addOnFailureListener {
                invoke.reject("Google Play 업데이트를 확인하지 못했어요. 잠시 후 다시 시도해 주세요.")
            }
        }, { invoke.reject("업데이트 서버에 연결하지 못했어요.") })
    }

    @Command fun permissions(invoke: Invoke) { invoke.resolve(JSObject()) }

    @Command fun install(invoke: Invoke) {
        val requested = invoke.parseArgs(UpdateArgs::class.java).versionCode
        host.runOnUiThread {
            if (flowActive) {
                invoke.reject("업데이트가 진행 중이에요.")
                return@runOnUiThread
            }
            flowActive = true
            stable({ release ->
                manager.appUpdateInfo.addOnSuccessListener { update ->
                    if (release?.getLong("versionCode") != requested || requested != update.availableVersionCode().toLong()) {
                        flowActive = false
                        invoke.reject("업데이트 정보가 바뀌었어요. 다시 확인해 주세요.")
                    } else {
                        start(update, invoke)
                    }
                }.addOnFailureListener {
                    flowActive = false
                    invoke.reject("Google Play 업데이트를 확인하지 못했어요. 잠시 후 다시 시도해 주세요.")
                }
            }, {
                flowActive = false
                invoke.reject("업데이트 서버에 연결하지 못했어요.")
            })
        }
    }

    private fun start(update: AppUpdateInfo, invoke: Invoke? = null) {
        if (host.isFinishing || host.isDestroyed || !update.isUpdateTypeAllowed(AppUpdateType.IMMEDIATE)) {
            flowActive = false
            lastError = "지금은 앱 안에서 업데이트할 수 없어요. 잠시 후 다시 시도해 주세요."
            invoke?.reject(lastError!!)
            return
        }
        flowActive = true
        lastError = null
        try {
            manager.startUpdateFlow(update, host, AppUpdateOptions.newBuilder(AppUpdateType.IMMEDIATE).build())
                .addOnSuccessListener { result ->
                    flowActive = false
                    when (result) {
                        Activity.RESULT_OK -> invoke?.resolve(JSObject().apply { put("state", "play-completed") })
                        Activity.RESULT_CANCELED -> invoke?.resolve(JSObject().apply { put("state", "cancelled") })
                        else -> failed(invoke)
                    }
                }.addOnFailureListener { failed(invoke) }
        } catch (_: Exception) { failed(invoke) }
    }

    private fun failed(invoke: Invoke?) {
        flowActive = false
        lastError = "Google Play에서 업데이트하지 못했어요. 다시 시도해 주세요."
        invoke?.reject(lastError!!)
    }

    private fun resume() {
        if (flowActive || resuming || host.isFinishing || host.isDestroyed) return
        resuming = true
        stable({ release ->
            manager.appUpdateInfo.addOnSuccessListener { update ->
                resuming = false
                if (!flowActive && release?.getLong("versionCode") == update.availableVersionCode().toLong()
                    && update.updateAvailability() == UpdateAvailability.DEVELOPER_TRIGGERED_UPDATE_IN_PROGRESS) start(update)
            }.addOnFailureListener { resuming = false }
        }, { resuming = false })
    }

    override fun load(webView: WebView) { host.runOnUiThread { resume() } }
    override fun onResume(activity: AppCompatActivity) { host = activity; resume() }
}
