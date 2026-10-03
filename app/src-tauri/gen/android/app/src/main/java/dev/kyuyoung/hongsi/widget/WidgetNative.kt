package dev.kyuyoung.hongsi.widget

import android.content.Context
import androidx.annotation.Keep
import dev.kyuyoung.hongsi.CredentialVault
import org.json.JSONObject

@Keep
internal class WidgetCredentials(private val context: Context) {
    private val vault = CredentialVault(context.applicationContext)
    fun load(): String? = vault.load()
    fun save(secret: String) = vault.save(secret)
    fun clear() {
        vault.clear()
        WidgetData.replace(context, "")
        android.os.Handler(android.os.Looper.getMainLooper()).post { Widgets.updateAll(context) }
    }
}

@Keep
internal object WidgetNative {
    init { System.loadLibrary("hongsi_lib") }
    private var ready = false
    @Synchronized fun prepare(context: Context) {
        if (!ready) { initialize(WidgetCredentials(context)); ready = true }
    }
    @JvmStatic private external fun initialize(vault: WidgetCredentials)
    @JvmStatic private external fun request(input: String): String
    fun api(context: Context, path: String, method: String = "GET", body: JSONObject? = null, owner: String = WidgetData.read(context).text("owner")): JSONObject {
        prepare(context)
        return JSONObject(request(JSONObject().put("method", method).put("path", path).put("body", body ?: JSONObject.NULL).put("owner", owner).toString()))
    }
}
