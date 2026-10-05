package dev.kyuyoung.hongsi

import android.os.Bundle
import android.content.Intent
import android.webkit.WebView
import java.lang.ref.WeakReference
import dev.kyuyoung.hongsi.widget.WidgetNavigation
import androidx.activity.enableEdgeToEdge

class MainActivity : TauriActivity() {
  override val handleBackNavigation: Boolean = true
  private var widgetWebView = WeakReference<WebView>(null)
  override fun onWebViewCreate(webView: WebView) { super.onWebViewCreate(webView); widgetWebView = WeakReference(webView) }
  private fun notifyWidgets() { widgetWebView.get()?.evaluateJavascript("window.dispatchEvent(new Event('hongsi-widget'))", null) }
  override fun onNewIntent(intent: Intent) {
    // A restored activity can receive the click before notification plugins load.
    setIntent(intent)
    WidgetNavigation.accept(intent)
    super.onNewIntent(intent)
    notifyWidgets()
  }
  override fun onResume() { super.onResume(); notifyWidgets() }

  override fun onCreate(savedInstanceState: Bundle?) {
    dev.kyuyoung.hongsi.widget.WidgetNative.prepare(applicationContext)
    enableEdgeToEdge()
    WidgetNavigation.accept(intent)
    super.onCreate(savedInstanceState)
  }
}
