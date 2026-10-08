package dev.kyuyoung.hongsi

import android.app.Activity
import android.content.Context
import android.graphics.Bitmap
import android.os.SystemClock
import android.util.Base64
import android.util.Log
import android.webkit.WebSettings
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import com.google.zxing.BarcodeFormat
import com.google.zxing.EncodeHintType
import com.google.zxing.qrcode.QRCodeWriter
import com.google.zxing.qrcode.decoder.ErrorCorrectionLevel
import org.json.JSONArray
import org.json.JSONObject
import java.io.ByteArrayOutputStream
import java.net.CookieManager
import java.net.CookiePolicy
import java.net.URI
import java.util.Locale
import java.util.UUID
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicLong
import javax.crypto.Cipher
import javax.crypto.SecretKeyFactory
import javax.crypto.spec.IvParameterSpec
import javax.crypto.spec.PBEKeySpec
import javax.crypto.spec.SecretKeySpec
import javax.net.ssl.HttpsURLConnection

internal class StudentCardFailure(val state: String, val explanation: String, val retryable: Boolean = false) : Exception()

internal class StudentCardTrace(val startedAt: Long, val rustPreparationMs: Long) {
    val id = sequence.incrementAndGet()
    fun record(stage: String, ms: Long, bytes: Int = 0, headerMs: Long = 0, readMs: Long = 0, parseMs: Long = 0) {
        Log.i("HongsiQrTiming", JSONObject().put("request", id).put("stage", stage).put("ms", ms)
            .put("bytes", bytes).put("headerMs", headerMs).put("readMs", readMs).put("parseMs", parseMs).toString())
    }
    companion object { private val sequence = AtomicLong() }
}

internal object StudentCardProtocol {
    const val BUNDLED_LOCALE_VERSION = "20261006173647"
    fun responseLimit(service: String): Int =
        if (service == "HCO0201S01") 8 * 1024 * 1024 else 1024 * 1024

    fun encryptPassword(password: String, challenge: String): ByteArray {
        require(challenge.length == 96 && challenge.substring(32).all { it in "0123456789abcdefABCDEF" })
        fun hex(value: String) = value.chunked(2).map { it.toInt(16).toByte() }.toByteArray()
        val spec = PBEKeySpec(challenge.take(32).toCharArray(), hex(challenge.substring(32, 64)), 1000, 256)
        val key = try { SecretKeyFactory.getInstance("PBKDF2WithHmacSHA1").generateSecret(spec).encoded } finally { spec.clearPassword() }
        val plaintext = password.toByteArray(Charsets.UTF_8)
        return try {
            val cipher = Cipher.getInstance("AES/CBC/PKCS5Padding")
            cipher.init(Cipher.ENCRYPT_MODE, SecretKeySpec(key, "AES"), IvParameterSpec(hex(challenge.substring(64))))
            cipher.doFinal(plaintext)
        } finally { key.fill(0); plaintext.fill(0) }
    }

    fun qrMatrix(value: String) = QRCodeWriter().encode(value, BarcodeFormat.QR_CODE, 0, 0,
        mutableMapOf<EncodeHintType, Any>(
            EncodeHintType.ERROR_CORRECTION to ErrorCorrectionLevel.M,
            EncodeHintType.MARGIN to 4,
        ).apply {
            if (value.any { it.code > 127 }) put(EncodeHintType.CHARACTER_SET, "UTF-8")
        })

    fun timerSeconds(raw: String?): Int {
        val value = raw?.toDoubleOrNull()
        if (value == null || value <= 0) return 30
        if (!value.isFinite() || value > 3600 || value < 1 || value % 1.0 != 0.0) {
            throw StudentCardFailure("unavailable", "학생증 QR 유효시간을 확인하지 못했어요.", true)
        }
        return value.toInt()
    }
}

internal class HeyoungSession(val owner: String, private val userAgent: String) {
    private val cookies = CookieManager(null, CookiePolicy.ACCEPT_ORIGINAL_SERVER)
    @Volatile private var cancelled = false
    @Volatile private var connection: HttpsURLConnection? = null
    var validForMs = 30_000L
        private set
    var localeVersion = ""
        private set

    fun cancel() { cancelled = true; connection?.disconnect(); cookies.cookieStore.removeAll() }
    private fun checkActive() { if (cancelled) throw StudentCardFailure("unavailable", "학생증 QR 창을 다시 열어 주세요.", true) }

    private fun call(trace: StudentCardTrace, service: String, data: JSONObject, screen: String = "newLogin"): JSONObject {
        val startedAt = SystemClock.elapsedRealtime()
        checkActive()
        val uri = URI("https://campus.heyoung.co.kr:18091/service/$service")
        val conn = uri.toURL().openConnection() as HttpsURLConnection
        connection = conn
        try {
            checkActive()
            conn.instanceFollowRedirects = false
            conn.connectTimeout = 15_000
            conn.readTimeout = 20_000
            conn.requestMethod = "POST"
            conn.doOutput = true
            conn.useCaches = false
            conn.setRequestProperty("User-Agent", userAgent)
            conn.setRequestProperty("Content-Type", "application/json;charset=UTF-8")
            conn.setRequestProperty("Accept", "application/json")
            conn.setRequestProperty("Origin", "https://campus.heyoung.co.kr:18091")
            conn.setRequestProperty("Referer", "https://campus.heyoung.co.kr:18091/static/")
            conn.setRequestProperty("X-Requested-With", "com.shinhan.heyoung")
            cookies.get(uri, emptyMap()).forEach { (key, values) -> conn.setRequestProperty(key, values.joinToString("; ")) }
            val body = JSONObject().put("REQ_COM", JSONObject()
                .put("serviceId", service).put("screenId", screen).put("langCd", "KO")
                .put("clientAgent", userAgent.lowercase(Locale.ROOT)).put("clientOS", "android").put("univCd", "HIUV"))
                .put("REQ_DAT", data).toString().toByteArray(Charsets.UTF_8)
            try { conn.outputStream.use { it.write(body) } } finally { body.fill(0) }
            val status = conn.responseCode
            val headersAt = SystemClock.elapsedRealtime()
            checkActive()
            if (status == 401 || status == 403) throw StudentCardFailure("authentication_required", "학생증 인증이 만료됐어요. 다시 시도해 주세요.")
            if (status !in 200..299) throw StudentCardFailure("unavailable", "헤이영 서버에 연결하지 못했어요. 잠시 후 다시 시도해 주세요.", true)
            cookies.put(uri, conn.headerFields.filterKeys { it != null })
            val raw = conn.inputStream.use { it.readStudentCardResponse(StudentCardProtocol.responseLimit(service)) }
            val readAt = SystemClock.elapsedRealtime()
            val response = try { JSONObject(raw.toString(Charsets.UTF_8)) } finally { raw.fill(0) }
            val parsedAt = SystemClock.elapsedRealtime()
            trace.record(service, parsedAt - startedAt, raw.size, headersAt - startedAt, readAt - headersAt, parsedAt - readAt)
            checkActive()
            if (response.optJSONObject("RES_COM")?.optString("tranState") != "Y" || response.optJSONObject("RES_ERR")?.optString("errorCode").orEmpty().isNotEmpty()) {
                throw StudentCardFailure("unavailable", "헤이영에서 요청을 처리하지 못했어요. 계정 상태를 확인한 후 다시 시도해 주세요.", true)
            }
            return response.optJSONObject("RES_DAT") ?: throw StudentCardFailure("unavailable", "학생증 응답을 확인하지 못했어요.", true)
        } finally { conn.disconnect(); connection = null }
    }

    fun login(password: String, deviceId: String, knownLocaleVersion: String, trace: StudentCardTrace) {
        val initial = call(trace, "HCO0201S01", JSONObject().put("univ", "HIUV").put("plat", "android")
            .put("code", "94").put("version", "1.6.9").put("versionSource", "TARGET")
            .put("locale", "").put("localeVersion", knownLocaleVersion).put("heyTalkPolicyVersion", ""), "AppRoot")
        localeVersion = initial.optString("localeVersion").take(128)
        val challenge = initial.optString("enckey")
        call(trace, "HCO0112S02", JSONObject().put("univ", "HIUV"))
        val encryptStarted = SystemClock.elapsedRealtime()
        val encrypted = Base64.encodeToString(StudentCardProtocol.encryptPassword(password, challenge), Base64.NO_WRAP)
        trace.record("encrypt", SystemClock.elapsedRealtime() - encryptStarted)
        val login = call(trace, "HCO0204S01", JSONObject().put("univ", "HIUV").put("idno", owner)
            .put("pass", encrypted).put("pass_type", "E")
            .put("iddi", "").put("mnuIddi", "").put("moco", deviceId).put("tokn", "X")
            .put("plat", "android").put("auto", false).put("cookies", JSONArray()).put("heyKey", "")
            .put("oldMoco", "").put("locale", "ko").put("code", challenge).put("isUse", true))
        if (login.optString("code") !in setOf("000", "002")) {
            throw StudentCardFailure("authentication_required", "헤이영에 로그인하지 못했어요. 학교 계정 로그인을 확인해 주세요.")
        }
        val profile = login.optJSONObject("data")
        if (!profile?.optString("idno").orEmpty().equals(owner, ignoreCase = true)) {
            throw StudentCardFailure("authentication_required", "학생증 계정이 현재 홍시 계정과 일치하지 않아요. 다시 로그인해 주세요.")
        }
        val timer = call(trace, "CMM0101S05", JSONObject()
            .put("code_grp", "MOBILE_TIMER").put("univ_code", "G.CODE"), "HCO0401P01")
        validForMs = StudentCardProtocol.timerSeconds(timer.optJSONArray("data")?.optJSONObject(0)?.optString("code2")) * 1000L
    }

    fun qr(trace: StudentCardTrace): JSObject {
        val started = SystemClock.elapsedRealtime()
        val data = call(trace, "HCO0501S03", JSONObject().put("univ", ""), "HCO0401P01")
        val renderStarted = SystemClock.elapsedRealtime()
        val value = data.optString("qrcode")
        if (value.isBlank() || value.length > 4096) throw StudentCardFailure("unavailable", "발급된 학생증 QR이 없어요. 헤이영에서 학생증 등록 상태를 확인해 주세요.", true)
        val matrix = StudentCardProtocol.qrMatrix(value)
        val scale = maxOf(1, 512 / matrix.width)
        val width = matrix.width * scale
        val pixels = IntArray(width * width) { i -> if (matrix[i % width / scale, i / width / scale]) android.graphics.Color.BLACK else android.graphics.Color.WHITE }
        val bitmap = Bitmap.createBitmap(pixels, width, width, Bitmap.Config.ARGB_8888)
        val output = ByteArrayOutputStream()
        val image = try {
            check(bitmap.compress(Bitmap.CompressFormat.PNG, 100, output))
            "data:image/png;base64," + Base64.encodeToString(output.toByteArray(), Base64.NO_WRAP)
        } finally { bitmap.recycle(); pixels.fill(0); output.close() }
        checkActive()
        val readyAt = SystemClock.elapsedRealtime()
        val remaining = validForMs - (readyAt - started)
        if (remaining <= 0) throw StudentCardFailure("unavailable", "QR 유효시간이 지났어요. 다시 시도해 주세요.", true)
        trace.record("render", readyAt - renderStarted)
        return JSObject().put("state", "ready").put("imageDataUrl", image).put("validForMs", remaining)
            .put("processingMs", trace.rustPreparationMs + readyAt - trace.startedAt)
            .put("timingId", trace.id)
    }
}

internal fun java.io.InputStream.readStudentCardResponse(limit: Int): ByteArray {
    val output = ByteArrayOutputStream()
    val buffer = ByteArray(8192)
    while (true) {
        val count = read(buffer)
        if (count < 0) break
        if (output.size() + count > limit) throw StudentCardFailure("unavailable", "헤이영 응답이 너무 커서 처리하지 못했어요. 앱 업데이트가 필요해요.")
        output.write(buffer, 0, count)
    }
    return output.toByteArray()
}

internal object StudentCardSessions {
    private const val MAX_IDLE_MS = 5 * 60 * 1000L
    private val lock = Any()
    private var view = ""
    private var session: HeyoungSession? = null
    private var reusable = false
    private var busy = false
    private var lastUsedAt = 0L
    private val closed = LinkedHashSet<String>()
    @Volatile var foreground = true

    fun close(id: String) = synchronized(lock) {
        closed.add(id)
        if (closed.size > 128) closed.remove(closed.first())
        if (view == id) {
            if (busy || !reusable) discard()
            view = ""
        }
    }
    private fun discard() { session?.cancel(); session = null; reusable = false; busy = false; lastUsedAt = 0 }
    fun reset() = synchronized(lock) {
        if (view.isNotEmpty()) close(view)
        discard(); view = ""
    }
    fun background() = synchronized(lock) {
        foreground = false
        if (view.isNotEmpty()) close(view)
    }
    fun invalidate(id: String) = synchronized(lock) {
        if (view == id) discard()
        close(id)
    }
    fun active(id: String) = synchronized(lock) { foreground && view == id && id !in closed }
    fun complete(id: String, nowMs: Long = SystemClock.elapsedRealtime()): Boolean = synchronized(lock) {
        if (!active(id)) return@synchronized false
        busy = false; reusable = true; lastUsedAt = nowMs
        true
    }
    fun acquire(id: String, owner: String, userAgent: String, fresh: Boolean, nowMs: Long = SystemClock.elapsedRealtime()): Pair<HeyoungSession, Boolean> = synchronized(lock) {
        if (!foreground || id in closed) throw StudentCardFailure("unavailable", "학생증 QR 창을 다시 열어 주세요.", true)
        if (!fresh) {
            val current = session
            if (view != id || current == null || current.owner != owner || busy) throw StudentCardFailure("authentication_required", "학생증 인증이 만료됐어요. 다시 시도해 주세요.")
            busy = true
            return@synchronized current to false
        }
        if (view.isNotEmpty()) close(view)
        val cached = session
        if (cached != null && reusable && cached.owner == owner && nowMs - lastUsedAt in 0..MAX_IDLE_MS) {
            view = id; busy = true
            return@synchronized cached to false
        }
        discard()
        val next = HeyoungSession(owner, userAgent)
        view = id; session = next; busy = true
        next to true
    }
}

@InvokeArg
class StudentCardArgs { lateinit var viewId: String; lateinit var owner: String; lateinit var password: String; var fresh: Boolean = false; var rustPreparationMs: Long = 0 }
@InvokeArg
class CloseStudentCardArgs { lateinit var viewId: String }
@InvokeArg
class StudentCardDisplayArgs { lateinit var viewId: String; var timingId: Long = 0; var elapsedMs: Long = 0; var remainingMs: Long = 0 }

@TauriPlugin
class StudentCardPlugin(private val host: Activity) : Plugin(host) {
    private val worker = Executors.newSingleThreadExecutor()
    private val userAgent = WebSettings.getDefaultUserAgent(host) + " Heyoung/1.6.9"

    @Command fun displayed(invoke: Invoke) {
        val args = invoke.parseArgs(StudentCardDisplayArgs::class.java)
        if (StudentCardSessions.active(args.viewId) && args.timingId > 0 && args.elapsedMs in 0..300_000 && args.remainingMs in 0..3_600_000) {
            Log.i("HongsiQrTiming", JSONObject().put("request", args.timingId).put("stage", "display")
                .put("ms", args.elapsedMs).put("remainingMs", args.remainingMs).toString())
        }
        invoke.resolve()
    }

    @Command fun close(invoke: Invoke) {
        StudentCardSessions.close(invoke.parseArgs(CloseStudentCardArgs::class.java).viewId)
        invoke.resolve()
    }
    @Command fun request(invoke: Invoke) {
        val startedAt = SystemClock.elapsedRealtime()
        val args = invoke.parseArgs(StudentCardArgs::class.java)
        val trace = StudentCardTrace(startedAt, args.rustPreparationMs)
        worker.execute {
            try {
                trace.record("account", args.rustPreparationMs)
                trace.record("queue", SystemClock.elapsedRealtime() - startedAt)
                val (session, login) = StudentCardSessions.acquire(args.viewId, args.owner, userAgent, args.fresh)
                trace.record(if (login) "new_session" else "reuse_session", 0)
                if (login) {
                    val prefs = host.getSharedPreferences("student-card", Context.MODE_PRIVATE)
                    val deviceId = prefs.getString("device-id", null) ?: UUID.randomUUID().toString().also { prefs.edit().putString("device-id", it).apply() }
                    session.login(args.password, deviceId,
                        prefs.getString("locale-version", StudentCardProtocol.BUNDLED_LOCALE_VERSION) ?: StudentCardProtocol.BUNDLED_LOCALE_VERSION, trace)
                    if (session.localeVersion.isNotEmpty()) prefs.edit().putString("locale-version", session.localeVersion).apply()
                }
                val result = session.qr(trace)
                if (!StudentCardSessions.complete(args.viewId)) throw StudentCardFailure("unavailable", "학생증 QR 창을 다시 열어 주세요.", true)
                trace.record("native_total", SystemClock.elapsedRealtime() - startedAt)
                invoke.resolve(result)
            } catch (e: StudentCardFailure) {
                StudentCardSessions.invalidate(args.viewId)
                trace.record("failed", SystemClock.elapsedRealtime() - startedAt)
                invoke.resolve(JSObject().put("state", e.state).put("message", e.explanation).put("retryable", e.retryable))
            } catch (_: Exception) {
                StudentCardSessions.invalidate(args.viewId)
                trace.record("failed", SystemClock.elapsedRealtime() - startedAt)
                invoke.resolve(JSObject().put("state", "unavailable").put("message", "학생증 QR을 불러오지 못했어요. 네트워크 연결을 확인하고 다시 시도해 주세요.").put("retryable", true))
            } finally { args.password = "" }
        }
    }
}
