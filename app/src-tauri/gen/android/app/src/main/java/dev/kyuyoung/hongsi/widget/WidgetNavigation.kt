package dev.kyuyoung.hongsi.widget

import android.content.Intent
import org.json.JSONObject

internal object WidgetNavigation {
    private var pending: JSONObject? = null
    var changed = false
    fun accept(intent: Intent) {
        if (intent.action != "dev.kyuyoung.hongsi.WIDGET_OPEN") return
        val kind = WidgetKind.entries.find { it.name == intent.getStringExtra("kind") } ?: return
        pending = JSONObject().put("kind", kind.name).put("detail", intent.getStringExtra("detail").orEmpty()).put("owner", intent.getStringExtra("owner").orEmpty())
        intent.action = Intent.ACTION_MAIN
    }
    fun take(context: android.content.Context): JSONObject = JSONObject().put("owner", WidgetData.read(context).text("owner")).put("receipts", org.json.JSONArray(WidgetData.read(context).array("pendingReceipts").filter { WidgetAttendanceSnapshot.valid(it) })).put("target", pending ?: JSONObject.NULL).put("changed", changed).also { pending = null; changed = false }
}
