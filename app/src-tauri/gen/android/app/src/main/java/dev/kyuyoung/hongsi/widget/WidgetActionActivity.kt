package dev.kyuyoung.hongsi.widget

import android.Manifest
import android.appwidget.AppWidgetManager
import android.content.ComponentName
import android.content.pm.PackageManager
import android.location.Location
import android.location.LocationManager
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.view.Gravity
import android.widget.LinearLayout
import android.widget.ProgressBar
import android.widget.TextView
import android.widget.Toast
import androidx.appcompat.app.AlertDialog
import androidx.appcompat.app.AppCompatActivity
import androidx.core.content.ContextCompat
import androidx.core.location.LocationManagerCompat
import android.os.CancellationSignal
import org.json.JSONArray
import org.json.JSONObject
import java.util.concurrent.ConcurrentHashMap

class WidgetActionActivity : AppCompatActivity() {
    companion object { private val running = ConcurrentHashMap.newKeySet<String>() }
    private var id = 0
    private lateinit var kind: WidgetKind
    private var owner = ""
    private var operation = ""
    private var taskKey = ""
    private var dialog: AlertDialog? = null
    private var cancellation: CancellationSignal? = null
    private var started = false
    private var acquired = false
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        id = intent.getIntExtra("widget", 0)
        kind = WidgetKind.entries.find { it.name == intent.getStringExtra("kind") } ?: run { finish(); return }
        owner = intent.getStringExtra("owner").orEmpty()
        operation = intent.getStringExtra("operation").orEmpty()
        if (id <= 0 || AppWidgetManager.getInstance(this).getAppWidgetInfo(id)?.provider != ComponentName(this, kind.receiver) || owner.isBlank() || owner != WidgetData.read(this).text("owner")) { finish(); return }
        if (savedInstanceState != null) { error("처리 결과를 새로고침해서 확인해 주세요."); return }
        taskKey = "$owner:$operation"
        if (!running.add(taskKey)) { finish(); return }
        acquired = true
        if (operation == "submit" && kind.attendance) {
            val state = WidgetData.state(this, id)
            if (state.text("mode") != "input" || state.text("code").length != 4) { error("출석번호 네 자리를 입력해 주세요."); return }
            if (System.currentTimeMillis() - state.optLong("inputAt") > 600_000) { error("출석 정보를 새로고침한 뒤 다시 입력해 주세요."); return }
            if (ContextCompat.checkSelfPermission(this, Manifest.permission.ACCESS_COARSE_LOCATION) != PackageManager.PERMISSION_GRANTED && ContextCompat.checkSelfPermission(this, Manifest.permission.ACCESS_FINE_LOCATION) != PackageManager.PERMISSION_GRANTED) {
                requestPermissions(arrayOf(Manifest.permission.ACCESS_FINE_LOCATION, Manifest.permission.ACCESS_COARSE_LOCATION), 41)
            } else locate()
        } else if (operation in listOf("extend", "endSeat") && kind == WidgetKind.SEAT) {
            dialog = AlertDialog.Builder(this).setTitle(if (operation == "extend") "이용 시간을 연장할까요?" else "퇴실할까요?")
                .setMessage(if (operation == "extend") "홍시에 기록된 이용 시간을 연장해요." else "홍시에 기록된 좌석 이용을 종료해요.")
                .setNegativeButton("취소") { _, _ -> finish() }.setPositiveButton(if (operation == "extend") "연장" else "퇴실") { _, _ -> perform(null) }
                .setOnCancelListener { finish() }.show()
        } else finish()
    }
    override fun onRequestPermissionsResult(requestCode: Int, permissions: Array<out String>, grantResults: IntArray) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
        if (requestCode == 41) {
            if (grantResults.any { it == PackageManager.PERMISSION_GRANTED }) locate() else error("출석하려면 위치 권한이 필요해요. 기기 설정에서 허용해 주세요.")
        }
    }
    private fun progress(message: String) {
        dialog?.dismiss()
        val row = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL; gravity = Gravity.CENTER_VERTICAL
            val gap = (24 * resources.displayMetrics.density).toInt(); setPadding(gap, gap, gap, gap)
            addView(ProgressBar(context), LinearLayout.LayoutParams(gap, gap))
            addView(TextView(context).apply { text = message; textSize = 16f; setPadding(gap, 0, 0, 0) })
        }
        dialog = AlertDialog.Builder(this).setView(row).setCancelable(false).show()
    }
    @Suppress("MissingPermission")
    private fun locate() {
        progress("위치를 확인하고 있어요")
        val manager = getSystemService(LocationManager::class.java)
        val providers = listOf("fused", LocationManager.GPS_PROVIDER, LocationManager.NETWORK_PROVIDER).filter { runCatching { manager.isProviderEnabled(it) }.getOrDefault(false) }
        if (providers.isEmpty()) { error("기기의 위치 기능을 켜 주세요."); return }
        val cached = providers.mapNotNull { runCatching { manager.getLastKnownLocation(it) }.getOrNull() }
            .filter { android.os.SystemClock.elapsedRealtimeNanos() - it.elapsedRealtimeNanos in 0..30_000_000_000L && it.accuracy <= 100 }
            .minByOrNull { it.accuracy }
        if (cached != null) { perform(cached); return }
        val signal = CancellationSignal(); cancellation = signal
        var answered = false
        Handler(Looper.getMainLooper()).postDelayed({ if (!answered) { answered = true; signal.cancel(); if (!isFinishing) error("위치를 확인하지 못했어요. 위치 기능을 확인한 뒤 다시 시도해 주세요.") } }, 15_000)
        try {
            LocationManagerCompat.getCurrentLocation(manager, providers.first(), signal, ContextCompat.getMainExecutor(this)) { location ->
                if (!answered) { answered = true; if (isFinishing) return@getCurrentLocation; if (location == null) error("위치를 확인하지 못했어요. 다시 시도해 주세요.") else perform(location) }
            }
        } catch (_: Exception) { answered = true; error("위치를 확인하지 못했어요. 기기 설정을 확인해 주세요.") }
    }
    private fun perform(location: Location?) {
        if (started) return
        started = true
        progress(if (operation == "submit") "출석을 확인하고 있어요" else "처리하고 있어요")
        val context = applicationContext
        val state = WidgetData.state(context, id)
        WidgetSync.worker.execute {
            val result = runCatching {
                if (WidgetData.read(context).text("owner") != owner) throw WidgetFailure(409, "계정이 변경됐어요. 다시 확인해 주세요.")
                if (operation == "submit") {
                    val target = state.optJSONObject("lecture") ?: throw WidgetFailure(400, "출석 정보를 새로고침해 주세요.")
                    val active = (WidgetNative.api(context, "/api/attendance/active", owner = owner).body() as JSONObject).array("items")
                    if (active.none { it.text("key") == target.text("key") }) throw WidgetFailure(409, "출석할 수 있는 시간이 아니에요. 새로고침해 주세요.")
                    val body = JSONObject().put("lectureKey", target.text("key")).put("code", state.text("code")).put("latitude", location!!.latitude).put("longitude", location.longitude)
                    val response = WidgetNative.api(context, "/api/attendance/submit", "POST", body, owner).body() as JSONObject
                    val receipt = response.optJSONObject("receipt")
                    if (receipt == null || !WidgetAttendanceSnapshot.valid(receipt) || receipt.getJSONObject("lecture").text("key") != target.text("key")) {
                        val message = response.text("message")
                        throw WidgetFailure(400, if (Regex("번호.*(일치|틀|확인)|잘못.*번호").containsMatchIn(message)) "출석번호를 확인해 주세요." else "학교 응답을 처리하지 못했습니다.")
                    }
                    if (WidgetData.read(context).text("owner") == owner) {
                        val pending = WidgetData.read(context).array("pendingReceipts").filter { WidgetAttendanceSnapshot.valid(it) && it.getJSONObject("lecture").text("key") != target.text("key") } + listOf(receipt)
                        WidgetData.merge(context, owner, JSONObject().put("pendingReceipts", JSONArray(pending)))
                        Widgets.ids(context, WidgetKind.ATTENDANCE).forEach { widget ->
                            val s = WidgetData.state(context, widget).put("mode", "ready").put("code", "")
                            s.put("localReceipt", JSONObject().put("identity", target.text("identity")).put("date", receipt.text("date")).put("kind", receipt.text("kind")))
                            WidgetData.save(context, widget, s)
                        }
                    }
                    "출석확인이 완료되었습니다."
                } else {
                    val current = (WidgetNative.api(context, "/api/seats/session", owner = owner).body() as JSONObject).optJSONObject("session")
                    if (current == null || current.optLong("id") != intent.getLongExtra("seat", 0)) throw WidgetFailure(409, "좌석 정보가 변경됐어요. 새로고침해 주세요.")
                    val path = if (operation == "extend") "/api/seats/session/extend" else "/api/seats/session/end"
                    val body = WidgetNative.api(context, path, "POST", owner = owner).body() as JSONObject
                    WidgetData.merge(context, owner, JSONObject().put("seat", body.opt("session") ?: JSONObject.NULL).put("updatedAt", JSONObject().put("seats", System.currentTimeMillis())).put("errors", JSONObject().put("seats", "")))
                    if (operation == "extend") "이용 시간을 연장했어요." else "퇴실했어요."
                }
            }
            running.remove(taskKey)
            WidgetSync.main.post {
                Widgets.updateAll(context, preserveInput = result.isFailure)
                WidgetSync.enqueue(context, kind)
                WidgetNavigation.changed = true
                result.onSuccess { Toast.makeText(context, it, Toast.LENGTH_SHORT).show(); if (!isFinishing) finish() }
                    .onFailure { if (!isFinishing) error((it as? WidgetFailure)?.message ?: "처리 결과를 확인하지 못했어요. 새로고침해서 확인해 주세요.") }
            }
        }
    }
    private fun error(message: String) {
        if (acquired) running.remove(taskKey)
        dialog?.dismiss()
        dialog = AlertDialog.Builder(this).setMessage(message).setPositiveButton("확인") { _, _ -> finish() }.setOnCancelListener { finish() }.show()
    }
    override fun onDestroy() {
        cancellation?.cancel()
        dialog?.dismiss()
        if (acquired && !started) running.remove(taskKey)
        super.onDestroy()
    }
}
