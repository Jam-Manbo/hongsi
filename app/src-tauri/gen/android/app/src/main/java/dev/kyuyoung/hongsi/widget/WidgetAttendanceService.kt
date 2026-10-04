package dev.kyuyoung.hongsi.widget

import android.Manifest
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.appwidget.AppWidgetManager
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.content.pm.ServiceInfo
import android.location.Location
import android.location.LocationManager
import android.net.Uri
import android.os.Build
import android.os.CancellationSignal
import android.os.IBinder
import android.os.SystemClock
import android.widget.Toast
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat
import androidx.core.location.LocationManagerCompat
import dev.kyuyoung.hongsi.R
import org.json.JSONArray
import org.json.JSONObject

class WidgetAttendanceService : Service() {
    companion object {
        private const val CHANNEL = "widget-attendance"
        private const val NOTIFICATION = 8601
        @Volatile private var ownerInProgress = ""
        internal fun pending(context: Context) = ownerInProgress.isNotBlank() && ownerInProgress == WidgetData.read(context).text("owner")
        internal fun intent(context: Context, id: Int): PendingIntent {
            val inputAt = WidgetData.state(context, id).optLong("inputAt")
            val intent = Intent(context, WidgetAttendanceService::class.java)
                .setData(Uri.parse("hongsi-widget://submit/$id/$inputAt"))
                .putExtra("widget", id).putExtra("inputAt", inputAt)
                .putExtra("owner", WidgetData.read(context).text("owner"))
            val flags = PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
            return if (Build.VERSION.SDK_INT >= 26) PendingIntent.getForegroundService(context, 0, intent, flags)
            else PendingIntent.getService(context, 0, intent, flags)
        }
    }
    private var owner = ""
    private var state = JSONObject()
    private var cancellation: CancellationSignal? = null
    private var locating = false
    private var working = false
    private var finished = false
    private val locationTimeout = Runnable {
        if (locating) finish("위치를 찾지 못했어요.")
    }
    override fun onBind(intent: Intent?): IBinder? = null
    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (ownerInProgress.isNotBlank()) return START_NOT_STICKY
        val id = intent?.getIntExtra("widget", 0) ?: 0
        try { foreground(id, location = false) }
        catch (_: Exception) {
            stopSelf(); return START_NOT_STICKY
        }
        if (finished) { stopForeground(STOP_FOREGROUND_REMOVE); stopSelf(); return START_NOT_STICKY }
        owner = intent?.getStringExtra("owner").orEmpty()
        if (id <= 0 || owner.isBlank() || owner != WidgetData.read(this).text("owner") ||
            AppWidgetManager.getInstance(this).getAppWidgetInfo(id)?.provider != ComponentName(this, AttendanceWidgetReceiver::class.java)) {
            stopForeground(STOP_FOREGROUND_REMOVE); stopSelf(); return START_NOT_STICKY
        }
        state = WidgetData.state(this, id)
        if (state.text("mode") != "input" || state.optLong("inputAt") != intent?.getLongExtra("inputAt", 0) ||
            System.currentTimeMillis() - state.optLong("inputAt") !in 0..600_000) {
            finish("위젯을 새로고침해 주세요."); return START_NOT_STICKY
        }
        if (!Regex("[0-9]{4}").matches(state.text("code"))) {
            finish("출석번호 네 자리를 입력해 주세요."); return START_NOT_STICKY
        }
        if (ContextCompat.checkSelfPermission(this, Manifest.permission.ACCESS_COARSE_LOCATION) != PackageManager.PERMISSION_GRANTED &&
            ContextCompat.checkSelfPermission(this, Manifest.permission.ACCESS_FINE_LOCATION) != PackageManager.PERMISSION_GRANTED) {
            finish("앱에서 위치 권한을 허용해 주세요."); return START_NOT_STICKY
        }
        try {
            foreground(id, location = true)
            ownerInProgress = owner
            refreshWidgets()
            locate()
        } catch (_: Exception) {
            finish("다시 시도해 주세요.")
        }
        return START_NOT_STICKY
    }
    private fun foreground(id: Int, location: Boolean) {
        if (Build.VERSION.SDK_INT >= 26) {
            getSystemService(NotificationManager::class.java).createNotificationChannel(
                NotificationChannel(CHANNEL, "위젯 출석 처리", NotificationManager.IMPORTANCE_LOW).apply {
                    setSound(null, null); enableVibration(false); setShowBadge(false)
                })
        }
        val notification = NotificationCompat.Builder(this, CHANNEL)
            .setSmallIcon(R.drawable.ic_notification).setContentTitle("출석을 확인하고 있어요")
            .setContentText("처리가 끝나면 자동으로 닫혀요.")
            .setContentIntent(Widgets.page(this, id, WidgetKind.ATTENDANCE))
            .setOngoing(true).setSilent(true).setOnlyAlertOnce(true)
            .setPriority(NotificationCompat.PRIORITY_LOW).build()
        ServiceCompat.startForeground(this, NOTIFICATION, notification,
            when {
                location && Build.VERSION.SDK_INT >= 29 -> ServiceInfo.FOREGROUND_SERVICE_TYPE_LOCATION
                Build.VERSION.SDK_INT >= 34 -> ServiceInfo.FOREGROUND_SERVICE_TYPE_SHORT_SERVICE
                else -> 0
            })
    }
    @Suppress("MissingPermission")
    private fun locate() {
        val manager = getSystemService(LocationManager::class.java)
        val providers = listOf("fused", LocationManager.GPS_PROVIDER, LocationManager.NETWORK_PROVIDER)
            .filter { runCatching { manager.isProviderEnabled(it) }.getOrDefault(false) }
        if (providers.isEmpty()) { finish("기기의 위치 기능을 켜 주세요."); return }
        val cached = providers.mapNotNull { runCatching { manager.getLastKnownLocation(it) }.getOrNull() }
            .filter { SystemClock.elapsedRealtimeNanos() - it.elapsedRealtimeNanos in 0..30_000_000_000L && it.hasAccuracy() && it.accuracy <= 100 }
            .minByOrNull { it.accuracy }
        if (cached != null) { perform(cached); return }
        locating = true
        val signal = CancellationSignal(); cancellation = signal
        WidgetSync.main.postDelayed(locationTimeout, 15_000)
        try {
            LocationManagerCompat.getCurrentLocation(manager, providers.first(), signal, ContextCompat.getMainExecutor(this)) { location ->
                if (!locating || finished) return@getCurrentLocation
                locating = false
                WidgetSync.main.removeCallbacks(locationTimeout)
                if (location == null) finish("위치를 찾지 못했어요.") else perform(location)
            }
        } catch (_: Exception) {
            finish("위치를 찾지 못했어요.")
        }
    }
    private fun perform(location: Location) {
        if (working || finished) return
        working = true
        val context = applicationContext
        WidgetSync.worker.execute {
            val result = runCatching {
                val target = state.optJSONObject("lecture") ?: throw WidgetFailure(400, "위젯을 새로고침해 주세요.")
                if (WidgetData.read(context).text("owner") != owner) throw WidgetFailure(409, "위젯을 새로고침해 주세요.")
                val active = (WidgetNative.api(context, "/api/attendance/active", owner = owner).body() as JSONObject).array("items")
                if (active.none { it.text("key") == target.text("key") }) throw WidgetFailure(409, "출석할 수 있는 시간이 아니에요.")
                if (WidgetData.read(context).text("owner") != owner) throw WidgetFailure(409, "위젯을 새로고침해 주세요.")
                val body = JSONObject().put("lectureKey", target.text("key")).put("code", state.text("code"))
                    .put("latitude", location.latitude).put("longitude", location.longitude)
                val response = WidgetNative.api(context, "/api/attendance/submit", "POST", body, owner).body() as JSONObject
                val receipt = response.optJSONObject("receipt")
                if (receipt == null || !WidgetAttendanceSnapshot.valid(receipt) || receipt.getJSONObject("lecture").text("key") != target.text("key")) {
                    throw WidgetFailure(400, if (Regex("번호.*(일치|틀|확인)|잘못.*번호").containsMatchIn(response.text("message"))) "출석번호를 확인해 주세요." else "학교 응답을 처리하지 못했습니다.")
                }
                receipt
            }
            WidgetSync.main.post {
                if (finished) return@post
                if (WidgetData.read(context).text("owner") != owner) {
                    finish("위젯을 새로고침해 주세요.")
                    return@post
                }
                result.onSuccess { receipt ->
                    val target = state.getJSONObject("lecture")
                    val pending = WidgetData.read(context).array("pendingReceipts").filter {
                        WidgetAttendanceSnapshot.valid(it) && it.getJSONObject("lecture").text("key") != target.text("key")
                    } + receipt
                    WidgetData.merge(context, owner, JSONObject().put("pendingReceipts", JSONArray(pending)))
                    Widgets.ids(context, WidgetKind.ATTENDANCE).forEach { widget ->
                        val s = WidgetData.state(context, widget).put("mode", "ready").put("code", "")
                        s.put("localReceipt", JSONObject().put("identity", target.text("identity")).put("date", receipt.text("date")).put("kind", receipt.text("kind")))
                        WidgetData.save(context, widget, s)
                    }
                    finish(when (receipt.text("kind")) {
                        "late" -> "지각으로 처리됐어요."
                        "excused" -> "공결로 처리됐어요."
                        else -> "출석 확인이 완료되었습니다."
                    })
                }.onFailure {
                    finish((it as? WidgetFailure)?.message ?: "위젯을 새로고침해 주세요.")
                }
                WidgetNavigation.changed = true
                WidgetSync.enqueue(context, WidgetKind.ATTENDANCE)
            }
        }
    }
    private fun refreshWidgets() {
        Widgets.ids(this, WidgetKind.ATTENDANCE).forEach { Widgets.update(this, it, WidgetKind.ATTENDANCE) }
    }
    private fun finish(message: String) {
        if (finished) return
        finished = true
        locating = false
        cancellation?.cancel()
        WidgetSync.main.removeCallbacks(locationTimeout)
        ownerInProgress = ""
        refreshWidgets()
        Toast.makeText(applicationContext, message, Toast.LENGTH_SHORT).show()
        stopForeground(STOP_FOREGROUND_REMOVE)
        stopSelf()
    }
    override fun onTimeout(startId: Int) { finish("위젯을 새로고침해 주세요.") }
    override fun onTimeout(startId: Int, fgsType: Int) { onTimeout(startId) }
    override fun onDestroy() {
        finished = true
        locating = false
        cancellation?.cancel()
        WidgetSync.main.removeCallbacks(locationTimeout)
        ownerInProgress = ""
        refreshWidgets()
        super.onDestroy()
    }
}
