package dev.kyuyoung.hongsi.widget

import android.appwidget.AppWidgetManager
import android.content.ComponentCallbacks
import android.content.ComponentName
import android.content.Context
import android.content.res.Configuration
import android.os.Build
import android.widget.RemoteViews

internal object WidgetTheme {
    private var observing = false
    private fun preferences(context: Context) = context.getSharedPreferences("widget-settings", Context.MODE_PRIVATE)
    fun mode(context: Context): String = preferences(context).getString("theme", "system") ?: "system"
    fun context(base: Context, mode: String = mode(base)): Context {
        if (mode == "system") return base.applicationContext
        val configuration = Configuration(base.resources.configuration)
        configuration.uiMode = (configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK.inv()) or
            if (mode == "dark") Configuration.UI_MODE_NIGHT_YES else Configuration.UI_MODE_NIGHT_NO
        return base.createConfigurationContext(configuration)
    }
    fun resource(context: Context, resource: Int): Int = when (mode(context)) {
        "light" -> WidgetThemeResources.themed(resource, false)
        "dark" -> WidgetThemeResources.themed(resource, true)
        else -> resource
    }
    fun layout(context: Context, resource: Int) = RemoteViews(context.packageName, resource(context, resource))
    fun set(context: Context, value: String) {
        require(value in setOf("system", "light", "dark"))
        observe(context)
        val preferences = preferences(context)
        val changed = !preferences.contains("theme") || mode(context) != value
        if (changed) preferences.edit().putString("theme", value).apply()
        if (Build.VERSION.SDK_INT >= 28 && preferences.getString("previewTheme", null) != value) {
            val manager = AppWidgetManager.getInstance(context)
            val metadata = if (value == "system") null else "dev.kyuyoung.hongsi.widget.$value"
            WidgetKind.entries.forEach { manager.updateAppWidgetProviderInfo(ComponentName(context, it.receiver), metadata) }
            preferences.edit().putString("previewTheme", value).apply()
        }
        if (changed) Widgets.updateAll(context)
    }
    fun observe(context: Context) {
        if (observing) return
        observing = true
        val app = context.applicationContext
        var night = app.resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK
        app.registerComponentCallbacks(object : ComponentCallbacks {
            override fun onConfigurationChanged(configuration: Configuration) {
                val next = configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK
                if (next != night) {
                    night = next
                    if (mode(app) == "system") Widgets.updateAll(app)
                }
            }
            override fun onLowMemory() {}
        })
    }
}
