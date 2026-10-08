package dev.kyuyoung.hongsi

import android.app.Activity
import android.content.Context
import android.webkit.WebSettings
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleOwner
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.util.UUID

@InvokeArg
class StudentCardValueArgs { var value: String = "" }

@TauriPlugin
class StudentCardPlugin(private val host: Activity) : Plugin(host) {
    @Command fun studentCardContext(invoke: Invoke) {
        host.runOnUiThread {
            try {
                val prefs = host.getSharedPreferences("student-card", Context.MODE_PRIVATE)
                val deviceId = prefs.getString("device-id", null)
                    ?: UUID.randomUUID().toString().also { prefs.edit().putString("device-id", it).apply() }
                invoke.resolve(JSObject()
                    .put("platform", "android")
                    .put("deviceId", deviceId)
                    .put("userAgent", WebSettings.getDefaultUserAgent(host) + " Heyoung/1.6.9")
                    .put("localeVersion", prefs.getString("locale-version", "") ?: "")
                    .put("active", (host as? LifecycleOwner)?.lifecycle?.currentState?.isAtLeast(Lifecycle.State.RESUMED) == true))
            } catch (_: Exception) { invoke.reject("학생증 QR을 준비하지 못했어요.") }
        }
    }

    @Command fun saveStudentCardLocale(invoke: Invoke) {
        val value = invoke.parseArgs(StudentCardValueArgs::class.java).value
        if (value.length <= 128 && value.isNotEmpty()) {
            host.getSharedPreferences("student-card", Context.MODE_PRIVATE).edit().putString("locale-version", value).apply()
        }
        invoke.resolve()
    }
}
