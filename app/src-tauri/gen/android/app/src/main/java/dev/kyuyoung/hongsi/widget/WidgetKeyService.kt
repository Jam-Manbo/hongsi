package dev.kyuyoung.hongsi.widget

import android.content.Context
import android.content.Intent
import android.os.Build
import android.widget.RemoteViews
import android.widget.RemoteViewsService
import dev.kyuyoung.hongsi.R

class WidgetKeyService : RemoteViewsService() {
    override fun onGetViewFactory(intent: Intent): RemoteViewsFactory = if (intent.getBooleanExtra("codeDisplay", false)) Code(applicationContext, intent.getIntExtra("widget", 0)) else Keys(applicationContext, intent)
    private class Code(val context: Context, val widgetId: Int) : RemoteViewsFactory {
        override fun onCreate() {}
        override fun onDataSetChanged() {}
        override fun onDestroy() {}
        override fun getCount() = 1
        override fun getViewTypeCount() = 1
        override fun hasStableIds() = true
        override fun getItemId(position: Int) = 0L
        override fun getLoadingView(): RemoteViews = getViewAt(0)
        override fun getViewAt(position: Int): RemoteViews = WidgetTheme.layout(context, R.layout.widget_code_value).apply {
            setOnClickFillInIntent(R.id.widget_code, Intent())
            setTextViewText(R.id.widget_code, WidgetData.state(context, widgetId).text("code").padEnd(4, '–').map { it.toString() }.joinToString(" "))
        }
    }
    private class Keys(val context: Context, intent: Intent) : RemoteViewsFactory {
        private val height = intent.getIntExtra("height", 204)
        private val keys = listOf("1", "2", "3", "4", "5", "6", "7", "8", "9", "0")
        override fun onCreate() {}
        override fun onDataSetChanged() {}
        override fun onDestroy() {}
        override fun getCount() = keys.size
        override fun getViewTypeCount() = 1
        override fun hasStableIds() = true
        override fun getItemId(position: Int) = position.toLong()
        override fun getLoadingView(): RemoteViews? = null
        override fun getViewAt(position: Int): RemoteViews {
            val key = keys[position]
            val rowHeight = ((height - 16 - 54) / 2) - 4
            return WidgetTheme.layout(context, R.layout.widget_key_cell).apply {
                setTextViewText(R.id.widget_key_cell, key)
                setInt(R.id.widget_key_cell, "setHeight", (rowHeight.coerceAtLeast(48) * context.resources.displayMetrics.density).toInt())
                setInt(R.id.widget_key_cell, "setBackgroundResource", WidgetTheme.resource(context, R.drawable.widget_key_cell_background))
                if (Build.VERSION.SDK_INT >= 31) setColor(R.id.widget_key_cell, "setTextColor", WidgetTheme.resource(context, R.color.widget_text)) else setTextColor(R.id.widget_key_cell, WidgetTheme.context(context).getColor(R.color.widget_text))
                setContentDescription(R.id.widget_key_cell, key)
                setOnClickFillInIntent(R.id.widget_key_cell, Intent().putExtra("operation", "digit:$key"))
            }
        }
    }
}
