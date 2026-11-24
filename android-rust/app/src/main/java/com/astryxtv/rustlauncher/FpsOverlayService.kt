package com.astryxtv.rustlauncher

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.graphics.Color
import android.graphics.PixelFormat
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.hardware.display.DisplayManager
import android.os.Build
import android.os.IBinder
import android.provider.Settings
import android.util.TypedValue
import android.view.Choreographer
import android.view.Display
import android.view.Gravity
import android.view.View
import android.view.WindowManager
import android.widget.LinearLayout
import android.widget.TextView
import kotlin.math.roundToInt

/**
 * Lightweight system-wide floating FPS pill overlay (top-right) visible across
 * the launcher and all other Android TV apps. Never steals D-Pad focus.
 */
class FpsOverlayService : Service() {

    companion object {
        private const val CHANNEL_ID = "astryx_fps_overlay"
        private const val NOTIF_ID = 1042
        @Volatile
        var isRunning: Boolean = false
            private set
    }

    private var windowManager: WindowManager? = null
    private var rootView: LinearLayout? = null
    private var dotView: View? = null
    private var fpsText: TextView? = null

    private var frameCount = 0
    private var windowStartNs = 0L
    private var running = false

    private val frameCallback = object : Choreographer.FrameCallback {
        override fun doFrame(frameTimeNanos: Long) {
            if (!running) return
            if (windowStartNs == 0L) {
                windowStartNs = frameTimeNanos
                frameCount = 0
            } else {
                frameCount++
                val elapsedNs = frameTimeNanos - windowStartNs
                if (elapsedNs >= 450_000_000L) {
                    val fps = (frameCount * 1_000_000_000.0 / elapsedNs.toDouble())
                        .roundToInt()
                        .coerceIn(1, 240)
                    updatePill(fps)
                    frameCount = 0
                    windowStartNs = frameTimeNanos
                }
            }
            Choreographer.getInstance().postFrameCallback(this)
        }
    }

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M && !Settings.canDrawOverlays(this)) {
            stopSelf()
            return START_NOT_STICKY
        }

        startMinimalForeground()

        if (rootView == null) {
            attachOverlay()
        }
        if (!running) {
            running = true
            isRunning = true
            windowStartNs = 0L
            frameCount = 0
            Choreographer.getInstance().postFrameCallback(frameCallback)
        }
        return START_STICKY
    }

    private fun startMinimalForeground() {
        try {
            val nm = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                val ch = NotificationChannel(
                    CHANNEL_ID,
                    "FPS Meter Overlay",
                    NotificationManager.IMPORTANCE_MIN
                ).apply {
                    setShowBadge(false)
                    setSound(null, null)
                }
                nm.createNotificationChannel(ch)
            }
            val builder = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                Notification.Builder(this, CHANNEL_ID)
            } else {
                @Suppress("DEPRECATION")
                Notification.Builder(this)
            }
            val notif = builder
                .setContentTitle("Astryx FPS Meter")
                .setContentText("Live FPS overlay active")
                .setSmallIcon(android.R.drawable.ic_menu_info_details)
                .setOngoing(true)
                .build()

            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
                startForeground(NOTIF_ID, notif, ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE)
            } else {
                startForeground(NOTIF_ID, notif)
            }
        } catch (_: Throwable) {
        }
    }

    private fun dp(v: Float): Int {
        return TypedValue.applyDimension(
            TypedValue.COMPLEX_UNIT_DIP,
            v,
            resources.displayMetrics
        ).roundToInt()
    }

    private fun attachOverlay() {
        val wm = getSystemService(Context.WINDOW_SERVICE) as WindowManager
        windowManager = wm

        val bg = GradientDrawable().apply {
            shape = GradientDrawable.RECTANGLE
            cornerRadius = dp(16f).toFloat()
            setColor(Color.argb(214, 12, 16, 28))
            setStroke(dp(1f).coerceAtLeast(1), Color.argb(56, 255, 255, 255))
        }

        val container = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            background = bg
            setPadding(dp(11f), dp(5f), dp(12f), dp(5f))
            elevation = dp(6f).toFloat()
        }

        val dotBg = GradientDrawable().apply {
            shape = GradientDrawable.OVAL
            setColor(Color.rgb(52, 211, 153))
        }
        val dot = View(this).apply {
            background = dotBg
            layoutParams = LinearLayout.LayoutParams(dp(7f), dp(7f)).apply {
                marginEnd = dp(6f)
            }
        }

        val tv = TextView(this).apply {
            val hz = currentRefreshHz()
            text = "$hz FPS · ${hz}Hz"
            setTextColor(Color.WHITE)
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 11.5f)
            typeface = Typeface.create("sans-serif-medium", Typeface.BOLD)
            includeFontPadding = false
        }

        container.addView(dot)
        container.addView(tv)

        val overlayType = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY
        } else {
            @Suppress("DEPRECATION")
            WindowManager.LayoutParams.TYPE_PHONE
        }

        val lp = WindowManager.LayoutParams(
            WindowManager.LayoutParams.WRAP_CONTENT,
            WindowManager.LayoutParams.WRAP_CONTENT,
            overlayType,
            WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or
                WindowManager.LayoutParams.FLAG_NOT_TOUCHABLE or
                WindowManager.LayoutParams.FLAG_NOT_TOUCH_MODAL or
                WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN,
            PixelFormat.TRANSLUCENT
        ).apply {
            gravity = Gravity.TOP or Gravity.CENTER_HORIZONTAL
            x = 0
            y = dp(14f)
        }

        try {
            wm.addView(container, lp)
            rootView = container
            dotView = dot
            fpsText = tv
        } catch (_: Throwable) {
            stopSelf()
        }
    }

    private fun currentRefreshHz(): Int {
        return try {
            val dm = getSystemService(Context.DISPLAY_SERVICE) as? DisplayManager
            val hz = dm?.getDisplay(Display.DEFAULT_DISPLAY)?.refreshRate ?: 60f
            hz.roundToInt().coerceIn(24, 240)
        } catch (_: Throwable) {
            60
        }
    }

    private fun updatePill(fps: Int) {
        val hz = currentRefreshHz()
        fpsText?.text = "$fps FPS · ${hz}Hz"
        val color = when {
            fps >= (hz - 6).coerceAtLeast(22) -> Color.rgb(52, 211, 153)
            fps >= (hz * 0.6f).roundToInt() -> Color.rgb(251, 191, 36)
            else -> Color.rgb(248, 113, 113)
        }
        (dotView?.background as? GradientDrawable)?.setColor(color)
    }

    override fun onDestroy() {
        running = false
        isRunning = false
        try {
            Choreographer.getInstance().removeFrameCallback(frameCallback)
        } catch (_: Throwable) {
        }
        rootView?.let { v ->
            try {
                windowManager?.removeViewImmediate(v)
            } catch (_: Throwable) {
            }
        }
        rootView = null
        dotView = null
        fpsText = null
        super.onDestroy()
    }
}
