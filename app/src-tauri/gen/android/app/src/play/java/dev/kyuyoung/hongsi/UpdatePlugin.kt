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

@InvokeArg
class UpdateArgs { var versionCode: Long = 0 }

@TauriPlugin
class UpdatePlugin(private var host: Activity) : Plugin(host) {
    private val manager = AppUpdateManagerFactory.create(host.applicationContext)
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

    @Command fun check(invoke: Invoke) {
        host.runOnUiThread {
            manager.appUpdateInfo.addOnSuccessListener { update ->
                val available = update.updateAvailability() == UpdateAvailability.UPDATE_AVAILABLE ||
                    update.updateAvailability() == UpdateAvailability.DEVELOPER_TRIGGERED_UPDATE_IN_PROGRESS
                lastError = null
                val result = info().apply {
                    put("configured", true)
                    put("release", if (available) JSObject().apply {
                        put("version", JSONObject.NULL)
                        put("versionCode", update.availableVersionCode())
                        put("size", update.totalBytesToDownload().takeIf { it > 0 } ?: JSONObject.NULL)
                        put("notes", "")
                    } else JSONObject.NULL)
                }
                invoke.resolve(result)
            }.addOnFailureListener {
                invoke.reject("Google Play 업데이트를 확인하지 못했어요. 잠시 후 다시 시도해 주세요.")
            }
        }
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
            manager.appUpdateInfo.addOnSuccessListener { update ->
                if (requested != update.availableVersionCode().toLong()) {
                    flowActive = false
                    invoke.reject("업데이트 정보가 바뀌었어요. 다시 확인해 주세요.")
                } else {
                    start(update, invoke)
                }
            }.addOnFailureListener {
                flowActive = false
                invoke.reject("Google Play 업데이트를 확인하지 못했어요. 잠시 후 다시 시도해 주세요.")
            }
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
        manager.appUpdateInfo.addOnSuccessListener { update ->
            resuming = false
            if (!flowActive && update.updateAvailability() == UpdateAvailability.DEVELOPER_TRIGGERED_UPDATE_IN_PROGRESS) start(update)
        }.addOnFailureListener { resuming = false }
    }

    override fun load(webView: WebView) { host.runOnUiThread { resume() } }
    override fun onResume(activity: AppCompatActivity) { host = activity; resume() }
}
