package com.astryxtv.rustlauncher

import android.app.ActivityManager
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
import android.net.TrafficStats
import android.os.Build
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import android.os.SystemClock
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
 * Lightweight system-wide top-center HUD pill overlay visible across the launcher
 * and all Android TV apps. Supports independently toggling FPS, Screen Hz,
 * live Network Activity (download/upload), and Free RAM. Never steals D-Pad focus.
 */
class FpsOverlayService : Service() {

    companion object {
        private const val CHANNEL_ID = "astryx_fps_overlay"
        private const val NOTIF_ID = 1042
        const val EXTRA_FLAGS = "overlay_flags"
        const val FLAG_FPS = 1
        const val FLAG_HZ = 2
        const val FLAG_NET = 4
        const val FLAG_RAM = 8
        const val DEFAULT_FLAGS = FLAG_FPS or FLAG_HZ or FLAG_NET

        @Volatile
        var isRunning: Boolean = false
            private set
    }

    private var windowManager: WindowManager? = null
    private var rootView: LinearLayout? = null
    private var dotView: View? = null
    private var hudText: TextView? = null

    private var metricFlags: Int = DEFAULT_FLAGS
    private var frameCount = 0
    private var windowStartNs = 0L
    private var latestFps = 60
    private var running = false

    private var lastNetTimeMs = 0L
    private var lastRxBytes = -1L
    private var lastTxBytes = -1L
    private var latestNetText = "↓ 0 KB/s"
    private var latestRamText = ""

    private val handler = Handler(Looper.getMainLooper())

    private val slowTicker = object : Runnable {
        override fun run() {
            if (!running) return
            sampleNetworkAndRam()
            if ((metricFlags and FLAG_FPS) == 0) {
                renderPill()
            }
            handler.postDelayed(this, 1000L)
        }
    }

    private val frameCallback = object : Choreographer.FrameCallback {
        override fun doFrame(frameTimeNanos: Long) {
            if (!running || (metricFlags and FLAG_FPS) == 0) return
            if (windowStartNs == 0L) {
                windowStartNs = frameTimeNanos
                frameCount = 0
            } else {
                frameCount++
                val elapsedNs = frameTimeNanos - windowStartNs
                if (elapsedNs >= 450_000_000L) {
                    latestFps = (frameCount * 1_000_000_000.0 / elapsedNs.toDouble())
                        .roundToInt()
                        .coerceIn(1, 240)
                    renderPill()
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

        val prevFlags = metricFlags
        metricFlags = intent?.getIntExtra(EXTRA_FLAGS, metricFlags) ?: metricFlags

        startMinimalForeground()

        if (rootView == null) {
            attachOverlay()
        }

        if (!running) {
            running = true
            isRunning = true
            lastNetTimeMs = SystemClock.elapsedRealtime()
            lastRxBytes = TrafficStats.getTotalRxBytes()
            lastTxBytes = TrafficStats.getTotalTxBytes()
            sampleNetworkAndRam()
            handler.removeCallbacks(slowTicker)
            handler.postDelayed(slowTicker, 1000L)
            if ((metricFlags and FLAG_FPS) != 0) {
                windowStartNs = 0L
                frameCount = 0
                Choreographer.getInstance().postFrameCallback(frameCallback)
            }
        } else if ((prevFlags and FLAG_FPS) == 0 && (metricFlags and FLAG_FPS) != 0) {
            windowStartNs = 0L
            frameCount = 0
            Choreographer.getInstance().postFrameCallback(frameCallback)
        }

        renderPill()
        return START_STICKY
    }

    private fun startMinimalForeground() {
        try {
            val nm = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                val ch = NotificationChannel(
                    CHANNEL_ID,
                    "HUD Performance Overlay",
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
                .setContentTitle("Astryx HUD Overlay")
                .setContentText("Live performance pill active")
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
            setPadding(dp(12f), dp(5f), dp(13f), dp(5f))
            elevation = dp(6f).toFloat()
        }

        val dotBg = GradientDrawable().apply {
            shape = GradientDrawable.OVAL
            setColor(Color.rgb(52, 211, 153))
        }
        val dot = View(this).apply {
            background = dotBg
            layoutParams = LinearLayout.LayoutParams(dp(7f), dp(7f)).apply {
                marginEnd = dp(7f)
            }
        }

        val tv = TextView(this).apply {
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
            hudText = tv
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

    private fun formatRate(bytesPerSec: Double): String {
        return when {
            bytesPerSec >= 1_048_576.0 -> String.format("%.1f MB/s", bytesPerSec / 1_048_576.0)
            bytesPerSec >= 1024.0 -> String.format("%.0f KB/s", bytesPerSec / 1024.0)
            else -> "0 KB/s"
        }
    }

    private fun sampleNetworkAndRam() {
        try {
            if ((metricFlags and FLAG_NET) != 0) {
                val now = SystemClock.elapsedRealtime()
                val rx = TrafficStats.getTotalRxBytes()
                val tx = TrafficStats.getTotalTxBytes()
                if (rx >= 0 && tx >= 0 && lastRxBytes >= 0 && lastTxBytes >= 0) {
                    val dt = ((now - lastNetTimeMs).coerceAtLeast(200L)) / 1000.0
                    val rxRate = ((rx - lastRxBytes).coerceAtLeast(0L)) / dt
                    val txRate = ((tx - lastTxBytes).coerceAtLeast(0L)) / dt
                    latestNetText = if (txRate >= 5120.0) {
                        "↓ ${formatRate(rxRate)} ↑ ${formatRate(txRate)}"
                    } else {
                        "↓ ${formatRate(rxRate)}"
                    }
                }
                lastNetTimeMs = now
                lastRxBytes = rx
                lastTxBytes = tx
            }
            if ((metricFlags and FLAG_RAM) != 0) {
                val am = getSystemService(Context.ACTIVITY_SERVICE) as? ActivityManager
                if (am != null) {
                    val mi = ActivityManager.MemoryInfo()
                    am.getMemoryInfo(mi)
                    val freeMb = (mi.availMem / (1024L * 1024L)).toInt()
                    latestRamText = "${freeMb} MB"
                }
            }
        } catch (_: Throwable) {
        }
    }

    private fun renderPill() {
        val rv = rootView ?: return
        if (metricFlags == 0) {
            rv.visibility = View.GONE
            return
        }
        rv.visibility = View.VISIBLE

        val hz = currentRefreshHz()
        val parts = ArrayList<String>(4)
        if ((metricFlags and FLAG_FPS) != 0) parts.add("$latestFps FPS")
        if ((metricFlags and FLAG_HZ) != 0) parts.add("${hz}Hz")
        if ((metricFlags and FLAG_NET) != 0) parts.add(latestNetText)
        if ((metricFlags and FLAG_RAM) != 0 && latestRamText.isNotEmpty()) parts.add("RAM $latestRamText")

        hudText?.text = parts.joinToString("  ·  ")

        val color = if ((metricFlags and FLAG_FPS) != 0) {
            when {
                latestFps >= (hz - 6).coerceAtLeast(22) -> Color.rgb(52, 211, 153)
                latestFps >= (hz * 0.6f).roundToInt() -> Color.rgb(251, 191, 36)
                else -> Color.rgb(248, 113, 113)
            }
        } else {
            Color.rgb(56, 189, 248)
        }
        (dotView?.background as? GradientDrawable)?.setColor(color)
    }

    override fun onDestroy() {
        running = false
        isRunning = false
        handler.removeCallbacks(slowTicker)
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
        hudText = null
        super.onDestroy()
    }
}
