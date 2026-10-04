package dev.kyuyoung.hongsi

import android.app.Activity
import android.content.ActivityNotFoundException
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.provider.DocumentsContract
import androidx.core.content.FileProvider
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin

@InvokeArg
class ExternalArgs { lateinit var value: String }

internal object ExternalIntents {
    fun url(value: String): Intent {
        val uri = Uri.parse(value)
        require(uri.scheme in listOf("https", "http") && !uri.host.isNullOrBlank() && value.length < 16384)
        return Intent(Intent.ACTION_VIEW, uri).addCategory(Intent.CATEGORY_BROWSABLE)
    }
    fun file(activity: android.content.Context, value: String): Intent {
        val file = DownloadFiles.checked(activity, value)
        val uri = FileProvider.getUriForFile(activity, "${activity.packageName}.fileprovider", file)
        return Intent(Intent.ACTION_VIEW).setDataAndType(uri, DownloadFiles.mime(file))
            .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
    }
    fun folder(activity: android.content.Context): Intent =
        Intent(Intent.ACTION_VIEW).setDataAndType(DownloadFiles.documentUri(activity), DocumentsContract.Document.MIME_TYPE_DIR)
            .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
    fun folderPicker(activity: android.content.Context): Intent = Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
        addCategory(Intent.CATEGORY_OPENABLE)
        type = "*/*"
        if (Build.VERSION.SDK_INT >= 26) putExtra(DocumentsContract.EXTRA_INITIAL_URI, DownloadFiles.documentUri(activity))
    }
}

@TauriPlugin
class ExternalPlugin(private val host: Activity) : Plugin(host) {
    @Command fun setWidgetTheme(invoke: Invoke) {
        val value = invoke.parseArgs(ExternalArgs::class.java).value
        host.runOnUiThread {
            try {
                dev.kyuyoung.hongsi.widget.WidgetTheme.set(host, value)
                invoke.resolve(JSObject())
            } catch (_: Exception) { invoke.reject("위젯 테마를 적용하지 못했어요.") }
        }
    }
    @Command fun syncWidget(invoke: Invoke) {
        val value = invoke.parseArgs(ExternalArgs::class.java).value
        host.runOnUiThread {
            try {
                dev.kyuyoung.hongsi.widget.WidgetData.replace(host, value)
                dev.kyuyoung.hongsi.widget.Widgets.updateAll(host, preserveInput = true)
                dev.kyuyoung.hongsi.widget.WidgetSync.schedule(host)
                invoke.resolve(JSObject())
            } catch (_: Exception) { invoke.reject("위젯 정보를 저장하지 못했어요.") }
        }
    }
    @Command fun takeWidgetIntent(invoke: Invoke) {
        host.runOnUiThread { invoke.resolve(JSObject(dev.kyuyoung.hongsi.widget.WidgetNavigation.take(host).toString())) }
    }
    private fun launch(invoke: Invoke, message: String, action: () -> Unit) {
        host.runOnUiThread {
            try { action(); invoke.resolve(JSObject()) }
            catch (_: ActivityNotFoundException) { invoke.reject(message) }
            catch (_: java.io.FileNotFoundException) { invoke.reject("파일이 없어요.") }
            catch (_: Exception) { invoke.reject("파일이나 링크를 열지 못했어요.") }
        }
    }
    @Command fun openUrl(invoke: Invoke) {
        val value = invoke.parseArgs(ExternalArgs::class.java).value
        launch(invoke, "링크를 열 브라우저가 없어요.") { host.startActivity(ExternalIntents.url(value)) }
    }
    @Command fun openFile(invoke: Invoke) {
        val value = invoke.parseArgs(ExternalArgs::class.java).value
        launch(invoke, "이 파일 형식을 열 수 있는 앱이 없어요.") { host.startActivity(ExternalIntents.file(host, value)) }
    }
    @Command fun revealFile(invoke: Invoke) {
        val value = invoke.parseArgs(ExternalArgs::class.java).value
        launch(invoke, "파일 앱을 열지 못했어요.") {
            DownloadFiles.checked(host, value)
            try { host.startActivity(ExternalIntents.folder(host)) }
            catch (_: ActivityNotFoundException) { host.startActivity(ExternalIntents.folderPicker(host)) }
        }
    }
}
