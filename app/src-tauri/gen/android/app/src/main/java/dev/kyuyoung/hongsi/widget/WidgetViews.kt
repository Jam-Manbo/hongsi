package dev.kyuyoung.hongsi.widget

import android.appwidget.AppWidgetManager
import android.content.Context
import android.content.Intent
import android.app.PendingIntent
import android.net.Uri
import android.content.res.Configuration
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.RectF
import android.graphics.Typeface
import android.os.Build
import android.text.SpannableString
import android.text.Spanned
import android.text.style.ForegroundColorSpan
import android.text.style.RelativeSizeSpan
import android.text.style.StyleSpan
import android.view.View
import android.widget.RemoteViews
import dev.kyuyoung.hongsi.R
import org.json.JSONObject
import kotlin.math.ceil
import kotlin.math.max
import kotlin.math.min

internal object WidgetViews {
    private fun color(context: Context, resource: Int) = context.getColor(resource)
    private fun layout(context: Context, resource: Int) = RemoteViews(context.packageName, resource)
    private fun RemoteViews.text(id: Int, value: String) = setTextViewText(id, value)
    private fun RemoteViews.action(context: Context, id: Int, kind: WidgetKind, view: Int, operation: String) = setOnClickPendingIntent(view, Widgets.pending(context, id, kind, operation))
    private fun RemoteViews.page(context: Context, id: Int, kind: WidgetKind, view: Int, detail: String = "") = setOnClickPendingIntent(view, Widgets.page(context, id, kind, detail))
    private fun RemoteViews.themeText(context: Context, view: Int, resource: Int) {
        if (Build.VERSION.SDK_INT >= 31) setColor(view, "setTextColor", resource)
        else setTextColor(view, color(context, resource))
    }
    private fun RemoteViews.primary(context: Context, title: String) { text(R.id.widget_primary, title); setContentDescription(R.id.widget_primary, title); themeText(context, R.id.widget_primary, R.color.widget_on_primary) }
    private fun keypad(context: Context, id: Int, kind: WidgetKind, height: Int): RemoteViews = layout(context, R.layout.widget_keypad).apply {
        page(context, id, kind, R.id.widget_root)
        val codeAdapter = Intent(context, WidgetKeyService::class.java).setData(Uri.parse("hongsi-widget://code/$id")).putExtra("widget", id).putExtra("codeDisplay", true)
        @Suppress("DEPRECATION")
        setRemoteAdapter(R.id.widget_code_host, codeAdapter)
        val adapter = Intent(context, WidgetKeyService::class.java).setData(Uri.parse("hongsi-widget://keys/$id/$height")).putExtra("height", height)
        @Suppress("DEPRECATION")
        setRemoteAdapter(R.id.widget_keys, adapter)
        val flags = PendingIntent.FLAG_UPDATE_CURRENT or if (Build.VERSION.SDK_INT >= 31) PendingIntent.FLAG_MUTABLE else 0
        setPendingIntentTemplate(R.id.widget_keys, PendingIntent.getBroadcast(context, 0,
            Intent(context, kind.receiver).setAction(Widgets.ACTION).setData(Uri.parse("hongsi-widget://keys-action/$id")).putExtra("widget", id).addFlags(Intent.FLAG_RECEIVER_FOREGROUND), flags))
        action(context, id, kind, R.id.widget_back, "back")
        action(context, id, kind, R.id.widget_erase, "erase")
        setOnClickPendingIntent(R.id.widget_submit, Widgets.actionPage(context, id, kind, "submit"))
    }
    fun render(context: Context, id: Int, kind: WidgetKind): RemoteViews {
        val state = WidgetData.state(context, id)
        val data = WidgetData.read(context)
        val resource = if (kind.deadlines) "calendar" else if (kind == WidgetKind.SEAT) "seats" else "timetable"
        val failed = WidgetData.error(context, resource).isNotBlank()
        val mode = state.text("mode", "ready")
        val options = if (id > 0) AppWidgetManager.getInstance(context).getAppWidgetOptions(id) else android.os.Bundle()
        val height = options.getInt(AppWidgetManager.OPTION_APPWIDGET_MAX_HEIGHT, kind.height)
        val width = options.getInt(AppWidgetManager.OPTION_APPWIDGET_MIN_WIDTH, 340)
        if (kind == WidgetKind.SEAT) return seatView(context, id, kind, state)
        if (kind.attendance && mode == "input" && data.length() > 0 && System.currentTimeMillis() - state.optLong("inputAt") < 600_000) return keypad(context, id, kind, height)
        val weekOnly = kind == WidgetKind.WEEK
        val view = layout(context, if (weekOnly) R.layout.widget_week else R.layout.widget_list_card)
        if (kind.attendance) return attendanceView(context, id, kind, state, height)
        view.page(context, id, kind, if (weekOnly) R.id.widget_body else R.id.widget_list_root)
        if (kind.deadlines) {
            val density = context.resources.displayMetrics.density
            val horizontal = (12 * density).toInt()
            val vertical = (6 * density).toInt()
            view.setViewPadding(R.id.widget_list_root, horizontal, vertical, horizontal, vertical)
        }
        if (!weekOnly) {
            view.text(R.id.widget_title, if (kind.deadlines) "다가오는 마감" else if (kind.today) "오늘 수업" else kind.title)
            view.page(context, id, kind, R.id.widget_title)
            refresh(context, id, kind, view)
        }
        view.removeAllViews(R.id.widget_body)
        if (data.length() == 0 || failed) {
            val title = if (data.length() == 0) "앱에서 로그인해 주세요" else if (failed) "정보를 불러오지 못했어요" else when {
                kind.today -> "오늘은 수업이 없어요"
                weekOnly -> "수업이 없어요"
                kind.deadlines -> "남은 일정이 없어요"
                else -> "출석할 수업이 없어요"
            }
            if (weekOnly) return weekStatus(context, id, kind, title)
            view.addView(R.id.widget_body, empty(context, title))
            return view
        }
        when (kind) {
            WidgetKind.ATTENDANCE -> Unit
            WidgetKind.TODAY, WidgetKind.TODAY_LARGE -> {
                val all = WidgetData.today(context, state)
                val selected = weekday()
                view.text(R.id.widget_title, if (selected == weekday()) "오늘 수업" else "${"월화수목금토일"[selected]}요일 수업")
                val remaining = if (selected == weekday()) all.filter { minutes(it.text("start")) + (it.optJSONArray("periods")?.length() ?: 1) * 60 > minuteOfDay() } else all
                if (remaining.isEmpty()) view.addView(R.id.widget_body, empty(context, if (all.isEmpty()) "오늘은 수업이 없어요" else "오늘 수업이 모두 끝났어요"))
                val list = layout(context, R.layout.widget_class_list)
                remaining.take(max(1, (height - 72) / 64)).forEachIndexed { index, slot ->
                    val current = selected == weekday() && minutes(slot.text("start")) <= minuteOfDay()
                    val row = row(context, slot.text("start"), slot.text("name"), slot.text("room", "강의실 미정"), if (current) "수업 중" else "예정", slot.text("color"))
                    row.page(context, id, kind, R.id.widget_row_root, "class:${slot.text("name")}")
                    row.setViewVisibility(R.id.row_separator, if (index == 0) View.GONE else View.VISIBLE)
                    row.themeText(context, R.id.row_time, if (current) R.color.widget_primary else R.color.widget_text)
                    row.setInt(R.id.row_badge, "setBackgroundResource", if (current) R.drawable.widget_badge_info else R.drawable.widget_badge_muted)
                    row.themeText(context, R.id.row_badge, if (current) R.color.widget_badge_info_text else R.color.widget_muted)
                    list.addView(R.id.class_rows, row)
                }
                if (remaining.isNotEmpty()) view.addView(R.id.widget_body, list)
            }
            WidgetKind.DEADLINES, WidgetKind.DEADLINES_LARGE -> {
                val items = WidgetData.deadlines(context)
                if (items.isEmpty()) view.addView(R.id.widget_body, empty(context, "남은 일정이 없어요"))
                val capacity = max(1, (height - 52) / 66)
                val visibleCount = if (items.size > capacity) max(1, (height - 72) / 66) else capacity
                val hiddenCount = max(0, items.size - visibleCount)
                view.setViewVisibility(R.id.widget_more, if (hiddenCount > 0) View.VISIBLE else View.GONE)
                view.text(R.id.widget_more, if (hiddenCount > 0) "+ ${hiddenCount}개의 마감" else "")
                items.take(visibleCount).forEach { item ->
                    val due = if (item.isNull("due")) 0 else item.optLong("due")
                    val difference = WidgetData.deadlineDay(context, due) - Math.floorDiv(nowSeconds() + 32400, 86400)
                    val deadline = if (due == 0L) "날짜 없음" else WidgetData.deadlineLabel(context, due)
                    val row = layout(context, R.layout.widget_deadline_row)
                    val accent = runCatching { Color.parseColor(item.text("color")) }.getOrDefault(color(context, R.color.widget_primary))
                    row.setInt(R.id.deadline_accent, "setColorFilter", accent)
                    row.text(R.id.row_title, item.text("title"))
                    row.text(R.id.row_subtitle, listOf(item.text("course"), deadline).filter(String::isNotBlank).joinToString("  "))
                    row.text(R.id.row_badge, if (due == 0L) "미정" else if (difference <= 0) "D-DAY" else "D-$difference")
                    row.themeText(context, R.id.row_badge, if (difference <= 0 && due != 0L) R.color.widget_error else R.color.widget_text)
                    val status = item.text("status").ifBlank { "상태 확인 필요" }
                    val tone = when (status) {
                        "제출 완료", "출석 인정", "완료" -> R.drawable.widget_badge_ok to R.color.widget_badge_ok_text
                        "미제출", "미시청", "부분 인정", "미완료" -> R.drawable.widget_badge_warn to R.color.widget_badge_warn_text
                        "마감 지남", "미인정" -> R.drawable.widget_badge_danger to R.color.widget_badge_danger_text
                        "시청 전" -> R.drawable.widget_badge_info to R.color.widget_badge_info_text
                        else -> R.drawable.widget_badge_muted to R.color.widget_badge_muted_text
                    }
                    row.text(R.id.row_status, status)
                    row.setInt(R.id.row_status, "setBackgroundResource", tone.first)
                    row.themeText(context, R.id.row_status, tone.second)
                    row.page(context, id, kind, R.id.widget_row_root, "deadline:${item.text("key")}")
                    view.addView(R.id.widget_body, row)
                }
            }
            WidgetKind.WEEK -> {
                if (WidgetData.slots(context).isEmpty()) return weekStatus(context, id, kind, "등록된 수업이 없어요")
                else {
                    val grid = layout(context, R.layout.widget_week_grid)
                    grid.setImageViewBitmap(R.id.widget_grid, weekBitmap(context, max(260, width - 16), max(160, height - 16)))
                    grid.page(context, id, kind, R.id.widget_grid, "week")
                    view.addView(R.id.widget_body, grid)
                }
            }
            WidgetKind.SEAT -> Unit
        }
        return view
    }
    private fun attendanceView(context: Context, id: Int, kind: WidgetKind, state: JSONObject, height: Int): RemoteViews {
        val value = AttendanceState.read(context, state)
        return layout(context, R.layout.widget_card).apply {
            page(context, id, kind, R.id.widget_root)
            text(R.id.widget_title, "빠른 출결")
            page(context, id, kind, R.id.widget_title)
            refresh(context, id, kind, this)
            removeAllViews(R.id.widget_body)
            val wrongCode = state.text("mode") == "error" && value.available
            val tone = if (wrongCode) R.color.widget_error else when (value.tone) {
                "success" -> R.color.widget_success
                "error" -> R.color.widget_error
                "warn" -> R.color.widget_badge_warn_text
                "primary" -> R.color.widget_primary
                else -> R.color.widget_muted
            }
            val content = layout(context, R.layout.widget_attendance_body).apply {
                text(R.id.attendance_course, value.title)
                text(R.id.attendance_message, if (wrongCode) "출석번호를 확인해 주세요" else value.message)
                text(R.id.attendance_detail, value.detail)
                themeText(context, R.id.attendance_message, tone)
                val compact = height < 184
                setViewVisibility(R.id.attendance_course, if (value.title.isBlank()) View.GONE else View.VISIBLE)
                setViewVisibility(R.id.attendance_detail, if (compact || value.detail.isBlank()) View.GONE else View.VISIBLE)
                if (height < 220) {
                    setInt(R.id.attendance_course, "setMaxLines", 1)
                    setInt(R.id.attendance_message, "setMaxLines", 1)
                }
                if (compact) setTextViewTextSize(R.id.attendance_message, android.util.TypedValue.COMPLEX_UNIT_SP, 15f)
                if (value.title.isBlank()) {
                    setInt(R.id.attendance_message, "setGravity", android.view.Gravity.CENTER)
                    setInt(R.id.attendance_detail, "setGravity", android.view.Gravity.CENTER)
                }
                page(context, id, kind, R.id.attendance_course)
                page(context, id, kind, R.id.attendance_message)
            }
            addView(R.id.widget_body, content)
            setViewVisibility(R.id.widget_primary_area, if (value.available) View.VISIBLE else View.GONE)
            primary(context, if (wrongCode) "다시 입력" else "출석하기")
            action(context, id, kind, R.id.widget_primary, "open")
        }
    }
    private fun refresh(context: Context, id: Int, kind: WidgetKind, view: RemoteViews) {
        view.action(context, id, kind, R.id.widget_refresh, "refresh")
        val active = Widgets.refreshing(kind)
        view.setViewVisibility(R.id.refresh_icon, if (active) View.INVISIBLE else View.VISIBLE)
        view.setViewVisibility(R.id.refresh_spinner, if (active) View.VISIBLE else View.GONE)
    }
    private fun status(context: Context, id: Int, kind: WidgetKind, message: String): RemoteViews = layout(context, R.layout.widget_status).apply {
        page(context, id, kind, R.id.widget_root)
        text(R.id.widget_empty, message)
        page(context, id, kind, R.id.widget_empty)
        refresh(context, id, kind, this)
    }
    private fun weekStatus(context: Context, id: Int, kind: WidgetKind, message: String): RemoteViews = status(context, id, kind, message).apply {
        setViewVisibility(R.id.widget_refresh, View.GONE)
    }
    private fun empty(context: Context, message: String): RemoteViews = layout(context, R.layout.widget_dashed_state).apply { text(R.id.widget_empty, message) }
    private fun seatView(context: Context, id: Int, kind: WidgetKind, state: JSONObject): RemoteViews {
        val failed = WidgetData.error(context, "seats").isNotBlank()
        val seat = if (failed) null else WidgetData.seat(context, state)
        if (seat == null) return status(context, id, kind, if (failed) "정보를 불러오지 못했어요" else if (WidgetData.read(context).length() == 0) "앱을 한 번 열어 주세요" else "이용 중인 좌석이 없어요")
        val view = layout(context, R.layout.widget_seat)
        view.page(context, id, kind, R.id.widget_root)
        val seconds = max(0L, seat.optLong("expiresAt") - nowSeconds())
        val left = ceil(seconds / 60.0).toInt()
        val hours = left / 60
        val minutes = left % 60
        val duration = if (hours == 0) "${minutes}분" else if (minutes == 0) "${hours}시간" else "${hours}시간 ${minutes}분"
        val remaining = "$duration 남음"
        val building = seat.text("buildingName")
        val code = seat.text("building").ifBlank { mapOf("제4공학관" to "T", "학생회관" to "G", "홍문관" to "R")[building].orEmpty() }
        val caption = if (code.isBlank()) building else "$building (${code}동)"
        val heading = SpannableString(caption).apply {
            setSpan(StyleSpan(Typeface.BOLD), 0, building.length, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
            if (caption.length > building.length) {
                setSpan(RelativeSizeSpan(14f / 18f), building.length, caption.length, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
                setSpan(ForegroundColorSpan(color(context, R.color.widget_muted)), building.length, caption.length, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
            }
        }
        view.setTextViewText(R.id.seat_building, heading)
        view.text(R.id.seat_room, seat.text("roomName").replace(Regex("제\\s*(\\d+)"), "제 $1"))
        view.text(R.id.seat_number, "${seat.text("seatNo")}${if (seat.text("seatNo").all(Char::isDigit)) "번" else ""}")
        view.setTextViewText(R.id.seat_remaining, SpannableString(remaining).apply {
            setSpan(RelativeSizeSpan(.8f), remaining.length - 2, remaining.length, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
        })
        refresh(context, id, kind, view)
        view.setOnClickPendingIntent(R.id.seat_extend, Widgets.actionPage(context, id, kind, "extend"))
        view.setOnClickPendingIntent(R.id.seat_end, Widgets.actionPage(context, id, kind, "endSeat"))
        view.page(context, id, kind, R.id.seat_number)
        view.page(context, id, kind, R.id.seat_building)
        view.page(context, id, kind, R.id.seat_room, "seat")
        return view
    }
    private fun row(context: Context, time: String, title: String, sub: String, badge: String, tint: String): RemoteViews = layout(context, R.layout.widget_row).apply {
        text(R.id.row_time, time); text(R.id.row_title, title); text(R.id.row_subtitle, sub); text(R.id.row_badge, badge)
        themeText(context, R.id.row_subtitle, R.color.widget_muted)
        val accent = runCatching { Color.parseColor(tint) }.getOrDefault(color(context, R.color.widget_primary))
        setInt(R.id.row_accent, "setColorFilter", accent)
    }
    fun weekBitmap(context: Context, width: Int, height: Int): Bitmap {
        val scale = 2f
        val bitmap = Bitmap.createBitmap((width * scale).toInt(), (height * scale).toInt(), Bitmap.Config.ARGB_8888)
        val canvas = Canvas(bitmap); canvas.scale(scale, scale)
        val paint = Paint(Paint.ANTI_ALIAS_FLAG)
        val slots = WidgetData.slots(context)
        val days = max(5, (slots.maxOfOrNull { it.optInt("weekday") } ?: 4) + 1)
        val starts = slots.map { minutes(it.text("start")) }
        val fit = WidgetData.read(context).optJSONObject("preferences")?.text("timetableDisplay", "fit") != "full" && slots.isNotEmpty()
        val first = starts.minOrNull() ?: 540
        val last = slots.maxOfOrNull { minutes(it.text("start")) + (it.optJSONArray("periods")?.length() ?: 1) * 60 } ?: 1080
        val start = (if (fit) first else min(540, first)) / 60 * 60
        val end = ((if (fit) last else max(1080, last)) + 59) / 60 * 60
        val left = 26f; val top = 24f
        val col = (width - left) / days; val unit = (height - top) / (end - start)
        val muted = color(context, R.color.widget_muted)
        paint.color = muted; paint.textSize = 12f; paint.typeface = Typeface.DEFAULT
        (0 until days).forEach { day -> paint.textAlign = Paint.Align.CENTER; canvas.drawText("월화수목금토일"[day].toString(), left + (day + .5f) * col, 14f, paint) }
        for (minute in start until end step 60) {
            val y = top + (minute - start) * unit
            paint.color = muted; paint.alpha = 55; paint.strokeWidth = .5f
            canvas.drawLine(left, y, width.toFloat(), y, paint)
            paint.alpha = 255; paint.textAlign = Paint.Align.LEFT; paint.textSize = 11f
            canvas.drawText("${minute / 60}", 1f, y + 10f, paint)
        }
        slots.forEach { slot ->
            val day = slot.optInt("weekday")
            val minute = minutes(slot.text("start"))
            val finish = minute + (slot.optJSONArray("periods")?.length() ?: 1) * 60
            val box = RectF(left + day * col + 2, top + (minute - start) * unit + 1, left + (day + 1) * col - 2, top + (finish - start) * unit - 2)
            val base = runCatching { Color.parseColor(slot.text("color")) }.getOrDefault(Color.rgb(88, 144, 222))
            val dark = context.resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK == Configuration.UI_MODE_NIGHT_YES
            fun blend(a: Int, b: Int, f: Float) = (a * (1 - f) + b * f).toInt()
            val factor = if (dark) .40f else .73f
            paint.color = Color.rgb(blend(Color.red(base), 255, factor), blend(Color.green(base), 255, factor), blend(Color.blue(base), 255, factor))
            paint.style = Paint.Style.FILL; canvas.drawRoundRect(box, 5f, 5f, paint)
            if (day == weekday() && minuteOfDay() in minute until finish) {
                paint.style = Paint.Style.STROKE; paint.color = color(context, R.color.widget_primary); paint.strokeWidth = 2f; canvas.drawRoundRect(box, 5f, 5f, paint); paint.style = Paint.Style.FILL
            }
            canvas.save(); canvas.clipRect(box)
            paint.color = Color.rgb(36, 43, 53); paint.textSize = if (days > 5) 11f else 12f; paint.typeface = Typeface.create("sans-serif", Typeface.BOLD)
            val maxLines = ((box.height() - 19) / 14).toInt().coerceIn(1, 3)
            var rest = slot.text("name"); var y = box.top + 14; var line = 0
            while (rest.isNotEmpty() && line < maxLines) {
                var n = paint.breakText(rest, true, box.width() - 8, null).coerceAtLeast(1)
                val last = line == maxLines - 1
                val clipped = last && n < rest.length
                if (clipped) while (n > 1 && paint.measureText(rest.take(n) + "…") > box.width() - 8) n--
                val text = rest.take(n); rest = rest.drop(n)
                canvas.drawText(text + if (clipped) "…" else "", box.left + 4, y, paint); y += 14; line++
            }
            paint.textSize = 10f; paint.typeface = Typeface.DEFAULT
            canvas.drawText(slot.text("room").take(10), box.left + 4, min(y + 2, box.bottom - 4), paint)
            canvas.restore()
        }
        return bitmap
    }
}
