package com.astryxtv.rustlauncher

import android.Manifest
import android.app.ActivityManager
import android.app.NativeActivity
import android.app.PendingIntent
import android.app.role.RoleManager
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.ApplicationInfo
import android.content.pm.PackageInstaller
import android.content.pm.PackageManager
import android.media.AudioAttributes
import android.media.AudioFormat
import android.media.AudioManager
import android.media.AudioRecord
import android.media.AudioTrack
import android.media.MediaRecorder
import android.speech.tts.TextToSpeech
import android.speech.tts.UtteranceProgressListener
import java.io.File
import java.io.FileInputStream
import java.net.Inet4Address
import java.net.NetworkInterface
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.Locale
import kotlin.math.sqrt
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.drawable.BitmapDrawable
import android.graphics.drawable.Drawable
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.net.NetworkRequest
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.Environment
import android.os.StatFs
import android.provider.Settings
import android.view.KeyEvent
import android.view.Surface
import android.view.SurfaceHolder
import android.view.View
import org.json.JSONArray
import org.json.JSONObject
import java.io.ByteArrayOutputStream
import kotlin.math.roundToInt

/**
 * Thin Android NativeActivity host for the Pure Native Rust OpenGL ES launcher.
 * Zero WebView. Only exposes Android PackageManager, ConnectivityManager,
 * RoleManager, and SharedPreferences to Rust via JNI.
 */
class RustLauncherActivity : NativeActivity() {

    companion object {
        init {
            try {
                System.loadLibrary("astryx_tv_rust")
            } catch (_: UnsatisfiedLinkError) {
            }
        }
    }

    private val prefs by lazy {
        getSharedPreferences("astryx_tv_rust_prefs", Context.MODE_PRIVATE)
    }

    private external fun nativeOnBackPressed()
    private external fun nativeOnMenuPressed()
    private external fun nativeOnHomePressed()
    private external fun nativeOnStatusChanged()
    private external fun nativeOnPackagesChanged()
    private external fun nativeOnPaused()
    private external fun nativeOnResumed()

    private var netCallback: ConnectivityManager.NetworkCallback? = null
    private var packageReceiver: BroadcastReceiver? = null

    private fun notifyStatusChanged() {
        try {
            nativeOnStatusChanged()
        } catch (_: UnsatisfiedLinkError) {
        }
    }

    private fun notifyPackagesChanged() {
        try {
            nativeOnPackagesChanged()
        } catch (_: UnsatisfiedLinkError) {
        }
    }

    private fun notifyPaused() {
        try {
            nativeOnPaused()
        } catch (_: UnsatisfiedLinkError) {
        }
    }

    private fun notifyResumed() {
        try {
            nativeOnResumed()
        } catch (_: UnsatisfiedLinkError) {
        }
    }

    override fun dispatchKeyEvent(event: KeyEvent): Boolean {
        if (event.keyCode == KeyEvent.KEYCODE_BACK) {
            if (event.action == KeyEvent.ACTION_DOWN && event.repeatCount == 0) {
                try {
                    nativeOnBackPressed()
                } catch (_: UnsatisfiedLinkError) {
                }
            }
            return true
        }
        if (event.keyCode == KeyEvent.KEYCODE_MENU) {
            if (event.action == KeyEvent.ACTION_DOWN && event.repeatCount == 0) {
                try {
                    nativeOnMenuPressed()
                } catch (_: UnsatisfiedLinkError) {
                }
            }
            return true
        }
        return super.dispatchKeyEvent(event)
    }

    @Suppress("DEPRECATION")
    override fun onBackPressed() {
        try {
            nativeOnBackPressed()
        } catch (_: UnsatisfiedLinkError) {
        }
    }

    private var activeRefreshRateHz: Float = 60f

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        prefer4kHighRefreshDisplayMode()
        enterImmersiveMode()
        registerNetworkWatcher()
        registerPackageWatcher()
    }

    override fun onResume() {
        super.onResume()
        notifyResumed()
        notifyStatusChanged()
    }

    override fun onNewIntent(intent: Intent?) {
        super.onNewIntent(intent)
        if (intent?.hasCategory(Intent.CATEGORY_HOME) == true) {
            try {
                nativeOnHomePressed()
            } catch (_: UnsatisfiedLinkError) {
            }
        }
    }

    override fun onPause() {
        rustStopVoiceSession()
        notifyPaused()
        super.onPause()
    }

    override fun onStop() {
        rustStopVoiceSession()
        notifyPaused()
        super.onStop()
    }

    override fun onDestroy() {
        rustStopVoiceSession()
        try {
            ttsEngine?.shutdown()
            ttsEngine = null
        } catch (_: Throwable) {
        }
        unregisterPackageWatcher()
        unregisterNetworkWatcher()
        super.onDestroy()
    }

    private fun registerPackageWatcher() {
        try {
            val filter = IntentFilter().apply {
                addAction(Intent.ACTION_PACKAGE_ADDED)
                addAction(Intent.ACTION_PACKAGE_REMOVED)
                addAction(Intent.ACTION_PACKAGE_REPLACED)
                addAction(Intent.ACTION_PACKAGE_CHANGED)
                addDataScheme("package")
            }
            val recv = object : BroadcastReceiver() {
                override fun onReceive(context: Context?, intent: Intent?) {
                    notifyPackagesChanged()
                }
            }
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                registerReceiver(recv, filter, Context.RECEIVER_EXPORTED)
            } else {
                @Suppress("UnspecifiedRegisterReceiverFlag")
                registerReceiver(recv, filter)
            }
            packageReceiver = recv
        } catch (_: Throwable) {
        }
    }

    private fun unregisterPackageWatcher() {
        try {
            val recv = packageReceiver ?: return
            unregisterReceiver(recv)
            packageReceiver = null
        } catch (_: Throwable) {
        }
    }

    private fun registerNetworkWatcher() {
        try {
            val cm = getSystemService(Context.CONNECTIVITY_SERVICE) as ConnectivityManager
            val req = NetworkRequest.Builder()
                .removeCapability(NetworkCapabilities.NET_CAPABILITY_NOT_VPN)
                .build()
            val cb = object : ConnectivityManager.NetworkCallback() {
                override fun onAvailable(network: Network) {
                    notifyStatusChanged()
                }
                override fun onLost(network: Network) {
                    notifyStatusChanged()
                }
                override fun onCapabilitiesChanged(network: Network, caps: NetworkCapabilities) {
                    notifyStatusChanged()
                }
            }
            cm.registerNetworkCallback(req, cb)
            netCallback = cb
        } catch (_: Throwable) {
        }
    }

    private fun unregisterNetworkWatcher() {
        try {
            val cb = netCallback ?: return
            val cm = getSystemService(Context.CONNECTIVITY_SERVICE) as ConnectivityManager
            cm.unregisterNetworkCallback(cb)
            netCallback = null
        } catch (_: Throwable) {
        }
    }

    override fun surfaceCreated(holder: SurfaceHolder) {
        super.surfaceCreated(holder)
        notifyResumed()
        applySurfaceFrameRate(holder)
    }

    override fun surfaceDestroyed(holder: SurfaceHolder) {
        notifyPaused()
        super.surfaceDestroyed(holder)
    }

    private fun applySurfaceFrameRate(holder: SurfaceHolder) {
        try {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                val targetHz = activeRefreshRateHz.coerceIn(60f, 120f)
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                    holder.surface.setFrameRate(
                        targetHz,
                        Surface.FRAME_RATE_COMPATIBILITY_DEFAULT,
                        Surface.CHANGE_FRAME_RATE_ONLY_IF_SEAMLESS
                    )
                } else {
                    holder.surface.setFrameRate(
                        targetHz,
                        Surface.FRAME_RATE_COMPATIBILITY_DEFAULT
                    )
                }
            }
        } catch (_: Throwable) {
        }
    }

    private fun prefer4kHighRefreshDisplayMode() {
        try {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                window.colorMode = android.content.pm.ActivityInfo.COLOR_MODE_WIDE_COLOR_GAMUT
            }
            val d = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) display else @Suppress("DEPRECATION") windowManager.defaultDisplay
            val modes = d?.supportedModes
            // Prefer highest resolution (4K), breaking ties with highest refresh rate up to 120Hz
            val best = modes
                ?.filter { it.refreshRate <= 121f }
                ?.maxWithOrNull(compareBy<android.view.Display.Mode>({ it.physicalWidth * it.physicalHeight }, { it.refreshRate }))
                ?: modes?.maxWithOrNull(compareBy<android.view.Display.Mode>({ it.physicalWidth * it.physicalHeight }, { it.refreshRate }))
            if (best != null) {
                activeRefreshRateHz = best.refreshRate.coerceIn(60f, 120f)
                val lp = window.attributes
                lp.preferredDisplayModeId = best.modeId
                lp.preferredRefreshRate = activeRefreshRateHz
                window.attributes = lp
            } else if (d != null) {
                activeRefreshRateHz = d.refreshRate.coerceIn(60f, 120f)
                val lp = window.attributes
                lp.preferredRefreshRate = activeRefreshRateHz
                window.attributes = lp
            }
        } catch (_: Throwable) {
        }
    }

    fun rustGetRefreshRate(): Float {
        return activeRefreshRateHz
    }

    fun rustSetFpsOverlay(enabled: Boolean, flags: Int): String {
        return try {
            val svcIntent = Intent(this, FpsOverlayService::class.java).apply {
                putExtra(FpsOverlayService.EXTRA_FLAGS, flags)
            }
            if (!enabled) {
                stopService(svcIntent)
                return "OFF"
            }
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M && !Settings.canDrawOverlays(this)) {
                try {
                    val permIntent = Intent(
                        Settings.ACTION_MANAGE_OVERLAY_PERMISSION,
                        Uri.parse("package:$packageName")
                    ).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                    startActivity(permIntent)
                } catch (_: Throwable) {
                }
                return "NEED_PERMISSION"
            }
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                startForegroundService(svcIntent)
            } else {
                startService(svcIntent)
            }
            "ON"
        } catch (t: Throwable) {
            "ERR:${t.message ?: t.javaClass.simpleName}"
        }
    }

    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        if (hasFocus) {
            enterImmersiveMode()
            notifyStatusChanged()
        }
    }

    @Suppress("DEPRECATION")
    private fun enterImmersiveMode() {
        window.decorView.systemUiVisibility = (
            View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY
                or View.SYSTEM_UI_FLAG_FULLSCREEN
                or View.SYSTEM_UI_FLAG_HIDE_NAVIGATION
                or View.SYSTEM_UI_FLAG_LAYOUT_STABLE
                or View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN
                or View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION
            )
    }

    // --- JNI methods called directly from Rust (rust/src/bridge.rs) ---

    fun rustGetInstalledApps(): String {
        val pm = packageManager
        val seen = LinkedHashMap<String, JSONObject>()

        fun collect(intent: Intent, isLeanback: Boolean) {
            val resolved = pm.queryIntentActivities(intent, 0)
            for (ri in resolved) {
                val ai = ri.activityInfo ?: continue
                val pkg = ai.packageName ?: continue
                if (pkg == packageName) continue
                if (seen.containsKey(pkg)) continue

                val label = try {
                    ri.loadLabel(pm).toString().trim().ifEmpty { pkg }
                } catch (_: Throwable) {
                    pkg
                }
                val appInfo = ai.applicationInfo
                val isSystem = (appInfo.flags and ApplicationInfo.FLAG_SYSTEM) != 0
                val versionName = try {
                    pm.getPackageInfo(pkg, 0).versionName ?: ""
                } catch (_: Throwable) {
                    ""
                }

                val obj = JSONObject()
                obj.put("packageName", pkg)
                obj.put("label", label)
                obj.put("category", if (isLeanback) "tv" else "app")
                obj.put("isLeanback", isLeanback)
                obj.put("isSystemApp", isSystem)
                obj.put("versionName", versionName)
                seen[pkg] = obj
            }
        }

        collect(Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_LEANBACK_LAUNCHER), true)
        collect(Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_LAUNCHER), false)

        val arr = JSONArray()
        for (obj in seen.values) arr.put(obj)
        return arr.toString()
    }

    fun rustGetAppIconRgba(pkg: String): ByteArray? {
        return try {
            val pm = packageManager
            val drawable: Drawable = try {
                pm.getApplicationIcon(pkg)
            } catch (_: Throwable) {
                pm.getApplicationBanner(pkg) ?: return null
            }

            val size = 256
            val bmp = Bitmap.createBitmap(size, size, Bitmap.Config.ARGB_8888)
            val canvas = Canvas(bmp)

            // Clean dark-glass circular backdrop so transparent/irregular icons never look cut off
            val bgPaint = android.graphics.Paint(android.graphics.Paint.ANTI_ALIAS_FLAG)
            bgPaint.color = android.graphics.Color.rgb(22, 28, 44)
            canvas.drawRect(0f, 0f, size.toFloat(), size.toFloat(), bgPaint)

            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O &&
                drawable is android.graphics.drawable.AdaptiveIconDrawable
            ) {
                // Adaptive icons use a 108dp canvas with a 66dp circular safe zone;
                // scale 1.25x centered so background fills edge-to-edge and logo sits cleanly inside!
                val inset = -(size * 0.14f).roundToInt()
                drawable.background?.let { bg ->
                    bg.setBounds(inset, inset, size - inset, size - inset)
                    bg.draw(canvas)
                }
                drawable.foreground?.let { fg ->
                    fg.setBounds(inset, inset, size - inset, size - inset)
                    fg.draw(canvas)
                }
            } else {
                // Render drawable to a temporary 1:1 bitmap to inspect whether corners are opaque or transparent
                val iw = drawable.intrinsicWidth.takeIf { it > 0 } ?: size
                val ih = drawable.intrinsicHeight.takeIf { it > 0 } ?: size
                val tmp = Bitmap.createBitmap(size, size, Bitmap.Config.ARGB_8888)
                val tmpCanvas = Canvas(tmp)
                val fitScale = minOf(size.toFloat() / iw.toFloat(), size.toFloat() / ih.toFloat())
                val fw = (iw * fitScale).roundToInt().coerceAtLeast(1)
                val fh = (ih * fitScale).roundToInt().coerceAtLeast(1)
                val fl = (size - fw) / 2
                val ft = (size - fh) / 2

                if (drawable is BitmapDrawable && drawable.bitmap != null) {
                    val paint = android.graphics.Paint(
                        android.graphics.Paint.ANTI_ALIAS_FLAG or android.graphics.Paint.FILTER_BITMAP_FLAG
                    )
                    tmpCanvas.drawBitmap(
                        drawable.bitmap,
                        null,
                        android.graphics.Rect(fl, ft, fl + fw, ft + fh),
                        paint
                    )
                } else {
                    drawable.setBounds(fl, ft, fl + fw, ft + fh)
                    drawable.draw(tmpCanvas)
                }

                // Sample mid-edges so both square AND rounded-square icons (e.g. VyseTV) fill the circle cleanly
                val mx = fl + fw / 2
                val my = ft + fh / 2
                val opaqueEdges = listOf(
                    android.graphics.Color.alpha(tmp.getPixel(mx, (ft + 8).coerceIn(0, size - 1))),
                    android.graphics.Color.alpha(tmp.getPixel(mx, (ft + fh - 9).coerceIn(0, size - 1))),
                    android.graphics.Color.alpha(tmp.getPixel((fl + 8).coerceIn(0, size - 1), my)),
                    android.graphics.Color.alpha(tmp.getPixel((fl + fw - 9).coerceIn(0, size - 1), my)),
                ).count { it >= 220 }

                val paint = android.graphics.Paint(
                    android.graphics.Paint.ANTI_ALIAS_FLAG or android.graphics.Paint.FILTER_BITMAP_FLAG
                )
                if (opaqueEdges >= 3) {
                    // Opaque square / rounded-square icon (TizenTube, VyseTV, Astryx TV): fill circle edge-to-edge
                    val cropScale = maxOf(size.toFloat() / fw.toFloat(), size.toFloat() / fh.toFloat()) * 1.12f
                    val dw = (fw * cropScale).roundToInt()
                    val dh = (fh * cropScale).roundToInt()
                    val dl = (size - dw) / 2
                    val dt = (size - dh) / 2
                    canvas.drawBitmap(
                        tmp,
                        android.graphics.Rect(fl, ft, fl + fw, ft + fh),
                        android.graphics.Rect(dl, dt, dl + dw, dt + dh),
                        paint
                    )
                } else {
                    // Transparent/irregular logo (e.g. Play Store): center with comfortable 14% padding
                    val pad = (size * 0.14f).roundToInt()
                    canvas.drawBitmap(
                        tmp,
                        android.graphics.Rect(0, 0, size, size),
                        android.graphics.Rect(pad, pad, size - pad, size - pad),
                        paint
                    )
                }
                tmp.recycle()
            }

            val buf = java.nio.ByteBuffer.allocate(size * size * 4)
            bmp.copyPixelsToBuffer(buf)
            bmp.recycle()
            buf.array()
        } catch (_: Throwable) {
            null
        }
    }

    fun rustLaunchApp(pkg: String): Boolean {
        return try {
            val pm = packageManager
            val intent = pm.getLeanbackLaunchIntentForPackage(pkg)
                ?: pm.getLaunchIntentForPackage(pkg)
                ?: return false
            intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            startActivity(intent)
            true
        } catch (_: Throwable) {
            false
        }
    }

    fun rustOpenAppInfo(pkg: String) {
        try {
            val intent = Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS).apply {
                data = Uri.parse("package:$pkg")
                addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            }
            startActivity(intent)
        } catch (_: Throwable) {
        }
    }

    fun rustOpenUrl(url: String): Boolean {
        return try {
            val intent = Intent(Intent.ACTION_VIEW, Uri.parse(url)).apply {
                addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            }
            startActivity(intent)
            true
        } catch (_: Throwable) {
            false
        }
    }

    fun rustUninstallApp(pkg: String) {
        try {
            val intent = Intent(Intent.ACTION_DELETE).apply {
                data = Uri.parse("package:$pkg")
                addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            }
            startActivity(intent)
        } catch (_: Throwable) {
            try {
                @Suppress("DEPRECATION")
                val fallback = Intent(Intent.ACTION_UNINSTALL_PACKAGE).apply {
                    data = Uri.parse("package:$pkg")
                    addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                }
                startActivity(fallback)
            } catch (_: Throwable) {
                rustOpenAppInfo(pkg)
            }
        }
    }

    fun rustOpenSystemSettings(section: String) {
        val action = when (section) {
            "network" -> Settings.ACTION_WIFI_SETTINGS
            "apps" -> Settings.ACTION_APPLICATION_SETTINGS
            "display" -> Settings.ACTION_DISPLAY_SETTINGS
            "bluetooth" -> Settings.ACTION_BLUETOOTH_SETTINGS
            "dream" -> Settings.ACTION_DREAM_SETTINGS
            else -> Settings.ACTION_SETTINGS
        }
        try {
            startActivity(Intent(action).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        } catch (_: Throwable) {
            try {
                startActivity(Intent(Settings.ACTION_SETTINGS).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
            } catch (_: Throwable) {
            }
        }
    }

    fun rustGetLocalIp(): String {
        return try {
            val ifaces = NetworkInterface.getNetworkInterfaces()
            while (ifaces != null && ifaces.hasMoreElements()) {
                val iface = ifaces.nextElement()
                if (!iface.isUp || iface.isLoopback) continue
                val name = iface.name.lowercase()
                if (name.contains("tun") || name.contains("ppp") || name.contains("nord")) continue
                val addrs = iface.inetAddresses
                while (addrs.hasMoreElements()) {
                    val addr = addrs.nextElement()
                    if (!addr.isLoopbackAddress && addr is Inet4Address) {
                        val ip = addr.hostAddress ?: continue
                        if (ip.startsWith("192.168.") || ip.startsWith("10.") || ip.startsWith("172.")) {
                            return ip
                        }
                    }
                }
            }
            "127.0.0.1"
        } catch (_: Throwable) {
            "127.0.0.1"
        }
    }

    fun rustIsMusicActive(): Boolean {
        return try {
            val am = getSystemService(Context.AUDIO_SERVICE) as AudioManager
            am.isMusicActive
        } catch (_: Throwable) {
            false
        }
    }

    fun rustMediaPlayPause() {
        try {
            val am = getSystemService(Context.AUDIO_SERVICE) as AudioManager
            val now = android.os.SystemClock.uptimeMillis()
            am.dispatchMediaKeyEvent(KeyEvent(now, now, KeyEvent.ACTION_DOWN, KeyEvent.KEYCODE_MEDIA_PLAY_PAUSE, 0))
            am.dispatchMediaKeyEvent(KeyEvent(now, now, KeyEvent.ACTION_UP, KeyEvent.KEYCODE_MEDIA_PLAY_PAUSE, 0))
        } catch (_: Throwable) {
        }
    }

    fun rustTriggerSleep() {
        try {
            val dream = Intent(Intent.ACTION_MAIN).apply {
                setClassName("com.android.systemui", "com.android.systemui.Somnambulator")
                addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            }
            startActivity(dream)
        } catch (_: Throwable) {
            rustOpenSystemSettings("dream")
        }
    }

    fun rustOpenTvInputs(): Boolean {
        val candidates = listOf(
            Intent("com.android.tv.action.OPEN_INPUTS"),
            Intent(Intent.ACTION_VIEW, Uri.parse("content://android.media.tv/channel")),
            Intent(Intent.ACTION_VIEW, Uri.parse("content://android.media.tv/passthrough")),
            Intent(Settings.ACTION_DISPLAY_SETTINGS)
        )
        for (i in candidates) {
            try {
                i.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                if (i.resolveActivity(packageManager) != null) {
                    startActivity(i)
                    return true
                }
            } catch (_: Throwable) {
            }
        }
        return false
    }

    fun rustExportBackup(json: String): String {
        return try {
            val dir = getExternalFilesDir(null) ?: filesDir
            val f = File(dir, "astryx_launcher_backup.json")
            f.writeText(json)
            f.absolutePath
        } catch (t: Throwable) {
            "ERR:${t.message ?: "export failed"}"
        }
    }

    fun rustImportBackup(): String {
        return try {
            val dir = getExternalFilesDir(null) ?: filesDir
            val f = File(dir, "astryx_launcher_backup.json")
            if (f.exists()) f.readText() else ""
        } catch (_: Throwable) {
            ""
        }
    }

    fun rustIsDefaultHome(): Boolean {
        return try {
            val intent = Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_HOME)
            val ri = packageManager.resolveActivity(intent, PackageManager.MATCH_DEFAULT_ONLY)
            ri?.activityInfo?.packageName == packageName
        } catch (_: Throwable) {
            false
        }
    }

    fun rustOpenHomeSettings(): Boolean {
        val candidates = mutableListOf<Intent>()
        candidates.add(Intent(Settings.ACTION_HOME_SETTINGS))
        candidates.add(Intent(Settings.ACTION_MANAGE_DEFAULT_APPS_SETTINGS))
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            try {
                val rm = getSystemService(RoleManager::class.java)
                if (rm != null && rm.isRoleAvailable(RoleManager.ROLE_HOME)) {
                    candidates.add(rm.createRequestRoleIntent(RoleManager.ROLE_HOME))
                }
            } catch (_: Throwable) {
            }
        }
        val pm = packageManager
        for (intent in candidates) {
            try {
                if (intent.resolveActivity(pm) != null) {
                    intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                    startActivity(intent)
                    return true
                }
            } catch (_: Throwable) {
            }
        }
        // Final fallback: Android TV system chooser for HOME
        return try {
            val homeIntent = Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_HOME)
            val chooser = Intent.createChooser(homeIntent, "Choose Default Home Launcher")
            chooser.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            startActivity(chooser)
            true
        } catch (_: Throwable) {
            false
        }
    }

    fun rustGetSystemStatus(): String {
        val obj = JSONObject()
        try {
            val cm = getSystemService(Context.CONNECTIVITY_SERVICE) as ConnectivityManager
            val net = cm.activeNetwork
            val caps = if (net != null) cm.getNetworkCapabilities(net) else null
            val connected = caps != null && caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET)
            val netType = when {
                caps == null -> "Offline"
                caps.hasTransport(NetworkCapabilities.TRANSPORT_ETHERNET) -> "Ethernet"
                caps.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) -> "Wi-Fi"
                else -> "Online"
            }
            var vpnActive = caps?.hasTransport(NetworkCapabilities.TRANSPORT_VPN) == true
            if (!vpnActive) {
                @Suppress("DEPRECATION")
                for (n in cm.allNetworks) {
                    val c = cm.getNetworkCapabilities(n)
                    if (c != null && c.hasTransport(NetworkCapabilities.TRANSPORT_VPN)) {
                        vpnActive = true
                        break
                    }
                }
            }

            val am = getSystemService(Context.ACTIVITY_SERVICE) as ActivityManager
            val mi = ActivityManager.MemoryInfo()
            am.getMemoryInfo(mi)
            val ramFreeMb = (mi.availMem / (1024L * 1024L)).toInt()
            val ramTotalMb = (mi.totalMem / (1024L * 1024L)).toInt()

            val stat = StatFs(Environment.getDataDirectory().path)
            val freeGb = ((stat.availableBytes.toDouble() / 1_073_741_824.0) * 10.0).roundToInt() / 10.0
            val totalGb = ((stat.totalBytes.toDouble() / 1_073_741_824.0) * 10.0).roundToInt() / 10.0

            obj.put("networkType", netType)
            obj.put("isConnected", connected)
            obj.put("isVpnActive", vpnActive)
            obj.put("deviceModel", Build.MODEL ?: "Android TV")
            obj.put("androidVersion", Build.VERSION.RELEASE ?: "12")
            obj.put("ramFreeMb", ramFreeMb)
            obj.put("ramTotalMb", ramTotalMb)
            obj.put("storageFreeGb", freeGb)
            obj.put("storageTotalGb", totalGb)
        } catch (_: Throwable) {
        }
        return obj.toString()
    }

    fun rustClearRam(): Int {
        return try {
            val am = getSystemService(Context.ACTIVITY_SERVICE) as ActivityManager
            val pm = packageManager
            val packages = pm.getInstalledApplications(0)
            for (app in packages) {
                val pkg = app.packageName ?: continue
                if (pkg == packageName) continue
                // Never kill active VPN services
                if (pkg.contains("vpn", ignoreCase = true)) continue
                try {
                    am.killBackgroundProcesses(pkg)
                } catch (_: Throwable) {
                }
            }
            Runtime.getRuntime().gc()
            val mi = ActivityManager.MemoryInfo()
            am.getMemoryInfo(mi)
            (mi.availMem / (1024L * 1024L)).toInt()
        } catch (_: Throwable) {
            0
        }
    }

    fun rustGetPref(key: String): String {
        return prefs.getString(key, "") ?: ""
    }

    fun rustSetPref(key: String, value: String) {
        prefs.edit().putString(key, value).apply()
    }

    fun rustGetAppVersion(): String {
        return try {
            packageManager.getPackageInfo(packageName, 0).versionName ?: "1.0.0"
        } catch (_: Throwable) {
            "1.0.0"
        }
    }

    fun rustGetCacheDir(): String {
        return cacheDir.absolutePath
    }

    fun rustInstallApk(apkPath: String): String {
        return try {
            val file = File(apkPath)
            if (!file.exists() || file.length() < 100_000L) {
                return "ERR:Downloaded APK file is missing or incomplete"
            }
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O && !packageManager.canRequestPackageInstalls()) {
                val permIntent = Intent(
                    Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES,
                    Uri.parse("package:$packageName")
                ).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                try {
                    startActivity(permIntent)
                } catch (_: Throwable) {
                }
                return "NEED_PERMISSION"
            }

            val installer = packageManager.packageInstaller
            val params = PackageInstaller.SessionParams(PackageInstaller.SessionParams.MODE_FULL_INSTALL)
            params.setSize(file.length())
            val sessionId = installer.createSession(params)
            val session = installer.openSession(sessionId)

            session.openWrite("astryx_update", 0, file.length()).use { out ->
                FileInputStream(file).use { input ->
                    val buf = ByteArray(65_536)
                    while (true) {
                        val n = input.read(buf)
                        if (n <= 0) break
                        out.write(buf, 0, n)
                    }
                }
                session.fsync(out)
            }

            val callbackIntent = Intent(this, UpdateInstallReceiver::class.java)
            val flags = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_MUTABLE
            } else {
                PendingIntent.FLAG_UPDATE_CURRENT
            }
            val pending = PendingIntent.getBroadcast(this, sessionId, callbackIntent, flags)
            session.commit(pending.intentSender)
            session.close()
            "OK"
        } catch (t: Throwable) {
            "ERR:${t.message ?: t.javaClass.simpleName}"
        }
    }

    // =========================================================================
    // Native Gemini Voice Capture (16kHz PCM -> WAV + VAD) & Expressive Audio Playback
    // =========================================================================

    @Volatile private var isRecordingVoice = false
    @Volatile private var voiceAutoStopped = false
    @Volatile private var currentMicLevel = 0f
    @Volatile private var recordedWavBytes: ByteArray = ByteArray(0)
    private var recordThread: Thread? = null

    @Volatile private var isPlayingSpeech = false
    @Volatile private var stopSpeechRequested = false
    @Volatile private var currentSpeakLevel = 0f
    private var activeAudioTrack: AudioTrack? = null
    private var ttsEngine: TextToSpeech? = null
    @Volatile private var ttsReady = false

    private fun ensureTts() {
        if (ttsEngine != null) return
        runOnUiThread {
            try {
                if (ttsEngine == null) {
                    ttsEngine = TextToSpeech(applicationContext) { status ->
                        if (status == TextToSpeech.SUCCESS) {
                            ttsEngine?.language = Locale.UK
                            ttsEngine?.setOnUtteranceProgressListener(object : UtteranceProgressListener() {
                                override fun onStart(utteranceId: String?) {
                                    isPlayingSpeech = true
                                    currentSpeakLevel = 0.55f
                                }
                                override fun onDone(utteranceId: String?) {
                                    isPlayingSpeech = false
                                    currentSpeakLevel = 0f
                                }
                                @Deprecated("Deprecated in Java")
                                override fun onError(utteranceId: String?) {
                                    isPlayingSpeech = false
                                    currentSpeakLevel = 0f
                                }
                            })
                            ttsReady = true
                        }
                    }
                }
            } catch (_: Throwable) {
            }
        }
    }

    fun rustStartVoiceRecording(): String {
        return try {
            if (checkSelfPermission(Manifest.permission.RECORD_AUDIO) != PackageManager.PERMISSION_GRANTED) {
                runOnUiThread {
                    try {
                        requestPermissions(arrayOf(Manifest.permission.RECORD_AUDIO), 901)
                    } catch (_: Throwable) {
                    }
                }
                return "NEED_MIC_PERMISSION"
            }
            rustStopSpeaking()
            if (isRecordingVoice) {
                return "OK"
            }

            val sampleRate = 16_000
            val minBuf = AudioRecord.getMinBufferSize(
                sampleRate,
                AudioFormat.CHANNEL_IN_MONO,
                AudioFormat.ENCODING_PCM_16BIT
            ).coerceAtLeast(3200)

            var recorder: AudioRecord? = null
            val sources = intArrayOf(
                MediaRecorder.AudioSource.VOICE_RECOGNITION,
                MediaRecorder.AudioSource.MIC,
                MediaRecorder.AudioSource.DEFAULT
            )
            for (src in sources) {
                try {
                    val r = AudioRecord(
                        src,
                        sampleRate,
                        AudioFormat.CHANNEL_IN_MONO,
                        AudioFormat.ENCODING_PCM_16BIT,
                        minBuf * 2
                    )
                    if (r.state == AudioRecord.STATE_INITIALIZED) {
                        recorder = r
                        break
                    } else {
                        r.release()
                    }
                } catch (_: Throwable) {
                }
            }
            val rec = recorder ?: return "ERR:No microphone found (connect BT mic/earbuds or use Phone Companion)"

            // Route Bluetooth headset/remote SCO mic if available
            val am = getSystemService(Context.AUDIO_SERVICE) as? AudioManager
            var scoStarted = false
            try {
                if (am != null && am.isBluetoothScoAvailableOffCall && !am.isBluetoothScoOn) {
                    @Suppress("DEPRECATION")
                    am.startBluetoothSco()
                    @Suppress("DEPRECATION")
                    am.isBluetoothScoOn = true
                    scoStarted = true
                }
            } catch (_: Throwable) {
            }

            recordedWavBytes = ByteArray(0)
            voiceAutoStopped = false
            currentMicLevel = 0f
            isRecordingVoice = true

            recordThread = Thread {
                val pcmOut = ByteArrayOutputStream(sampleRate * 2 * 8)
                val shortBuf = ShortArray(1024)
                val byteChunk = ByteArray(2048)
                var heardSpeech = false
                var speechFrames = 0
                var silenceSince = 0L
                val startedAt = System.currentTimeMillis()

                try {
                    rec.startRecording()
                    while (isRecordingVoice) {
                        val n = rec.read(shortBuf, 0, shortBuf.size)
                        if (n > 0) {
                            var sumSq = 0.0
                            for (i in 0 until n) {
                                val s = shortBuf[i].toInt()
                                byteChunk[i * 2] = (s and 0xFF).toByte()
                                byteChunk[i * 2 + 1] = ((s shr 8) and 0xFF).toByte()
                                val norm = s / 32768.0
                                sumSq += norm * norm
                            }
                            pcmOut.write(byteChunk, 0, n * 2)

                            val rms = sqrt(sumSq / n).toFloat()
                            val boosted = (rms * 5.2f).coerceIn(0f, 1f)
                            currentMicLevel = currentMicLevel * 0.55f + boosted * 0.45f

                            val now = System.currentTimeMillis()
                            if (boosted > 0.11f) {
                                speechFrames++
                                if (speechFrames >= 4) {
                                    heardSpeech = true
                                }
                                silenceSince = 0L
                            } else if (heardSpeech) {
                                if (silenceSince == 0L) {
                                    silenceSince = now
                                } else if (now - silenceSince >= 1250L) {
                                    voiceAutoStopped = true
                                    isRecordingVoice = false
                                    break
                                }
                            }
                            if (now - startedAt >= 12_000L) {
                                voiceAutoStopped = true
                                isRecordingVoice = false
                                break
                            }
                        } else {
                            Thread.sleep(15)
                        }
                    }
                } catch (_: Throwable) {
                } finally {
                    currentMicLevel = 0f
                    isRecordingVoice = false
                    try {
                        rec.stop()
                    } catch (_: Throwable) {
                    }
                    try {
                        rec.release()
                    } catch (_: Throwable) {
                    }
                    if (scoStarted) {
                        try {
                            @Suppress("DEPRECATION")
                            am?.isBluetoothScoOn = false
                            @Suppress("DEPRECATION")
                            am?.stopBluetoothSco()
                        } catch (_: Throwable) {
                        }
                    }
                    val pcm = pcmOut.toByteArray()
                    if (pcm.size >= 1600) {
                        recordedWavBytes = wrapPcmAsWav(pcm, sampleRate, 1, 16)
                    }
                }
            }.apply {
                name = "astryx-voice-rec"
                isDaemon = true
                start()
            }
            ensureTts()
            "OK"
        } catch (t: Throwable) {
            isRecordingVoice = false
            "ERR:${t.message ?: t.javaClass.simpleName}"
        }
    }

    fun rustStopVoiceRecording(): ByteArray {
        isRecordingVoice = false
        try {
            recordThread?.join(600)
        } catch (_: Throwable) {
        }
        recordThread = null
        currentMicLevel = 0f
        voiceAutoStopped = false
        val out = recordedWavBytes
        recordedWavBytes = ByteArray(0)
        return out
    }

    fun rustIsVoiceRecording(): Boolean = isRecordingVoice

    fun rustIsVoiceAutoStopped(): Boolean = voiceAutoStopped

    fun rustGetVoiceLevel(): Float = currentMicLevel

    private fun wrapPcmAsWav(pcm: ByteArray, sampleRate: Int, channels: Int, bitsPerSample: Int): ByteArray {
        val byteRate = sampleRate * channels * (bitsPerSample / 8)
        val blockAlign = (channels * (bitsPerSample / 8)).toShort()
        val out = ByteBuffer.allocate(44 + pcm.size).order(ByteOrder.LITTLE_ENDIAN)
        out.put("RIFF".toByteArray(Charsets.US_ASCII))
        out.putInt(36 + pcm.size)
        out.put("WAVE".toByteArray(Charsets.US_ASCII))
        out.put("fmt ".toByteArray(Charsets.US_ASCII))
        out.putInt(16)
        out.putShort(1) // PCM
        out.putShort(channels.toShort())
        out.putInt(sampleRate)
        out.putInt(byteRate)
        out.putShort(blockAlign)
        out.putShort(bitsPerSample.toShort())
        out.put("data".toByteArray(Charsets.US_ASCII))
        out.putInt(pcm.size)
        out.put(pcm)
        return out.array()
    }

    fun rustPlayWavBytes(wav: ByteArray): Boolean {
        if (wav.size < 48) return false
        return try {
            rustStopSpeaking()
            val bb = ByteBuffer.wrap(wav).order(ByteOrder.LITTLE_ENDIAN)
            if (wav[0] != 'R'.code.toByte() || wav[1] != 'I'.code.toByte()) return false
            var pos = 12
            var sampleRate = 24_000
            var channels = 1
            var dataStart = -1
            var dataLen = 0
            while (pos + 8 <= wav.size) {
                val c0 = wav[pos].toInt().toChar()
                val c1 = wav[pos + 1].toInt().toChar()
                val c2 = wav[pos + 2].toInt().toChar()
                val c3 = wav[pos + 3].toInt().toChar()
                val chunkSize = bb.getInt(pos + 4)
                val chunkData = pos + 8
                if (c0 == 'f' && c1 == 'm' && c2 == 't' && c3 == ' ' && chunkData + 16 <= wav.size) {
                    channels = bb.getShort(chunkData + 2).toInt().coerceIn(1, 2)
                    sampleRate = bb.getInt(chunkData + 4).coerceIn(8_000, 48_000)
                } else if (c0 == 'd' && c1 == 'a' && c2 == 't' && c3 == 'a') {
                    dataStart = chunkData
                    dataLen = chunkSize.coerceAtMost(wav.size - chunkData)
                    break
                }
                if (chunkSize < 0) break
                pos = chunkData + chunkSize + (chunkSize and 1)
            }
            if (dataStart < 0 || dataLen <= 0) return false

            val chMask = if (channels == 2) AudioFormat.CHANNEL_OUT_STEREO else AudioFormat.CHANNEL_OUT_MONO
            val minBuf = AudioTrack.getMinBufferSize(sampleRate, chMask, AudioFormat.ENCODING_PCM_16BIT)
                .coerceAtLeast(4096)

            val track = AudioTrack.Builder()
                .setAudioAttributes(
                    AudioAttributes.Builder()
                        .setUsage(AudioAttributes.USAGE_MEDIA)
                        .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH)
                        .build()
                )
                .setAudioFormat(
                    AudioFormat.Builder()
                        .setEncoding(AudioFormat.ENCODING_PCM_16BIT)
                        .setSampleRate(sampleRate)
                        .setChannelMask(chMask)
                        .build()
                )
                .setBufferSizeInBytes(minBuf * 2)
                .setTransferMode(AudioTrack.MODE_STREAM)
                .build()

            activeAudioTrack = track
            stopSpeechRequested = false
            isPlayingSpeech = true

            Thread {
                try {
                    track.play()
                    var offset = dataStart
                    val end = dataStart + dataLen
                    val step = 2048
                    while (offset < end && !stopSpeechRequested) {
                        val n = (end - offset).coerceAtMost(step)
                        var sumSq = 0.0
                        val samples = n / 2
                        if (samples > 0) {
                            for (i in 0 until samples) {
                                val lo = wav[offset + i * 2].toInt() and 0xFF
                                val hi = wav[offset + i * 2 + 1].toInt()
                                val s = ((hi shl 8) or lo).toShort().toInt()
                                val norm = s / 32768.0
                                sumSq += norm * norm
                            }
                            val rms = sqrt(sumSq / samples).toFloat()
                            currentSpeakLevel = currentSpeakLevel * 0.5f + (rms * 4.2f).coerceIn(0.08f, 1f) * 0.5f
                        }
                        val written = track.write(wav, offset, n)
                        if (written <= 0) break
                        offset += written
                    }
                } catch (_: Throwable) {
                } finally {
                    currentSpeakLevel = 0f
                    isPlayingSpeech = false
                    try {
                        track.stop()
                    } catch (_: Throwable) {
                    }
                    try {
                        track.release()
                    } catch (_: Throwable) {
                    }
                    if (activeAudioTrack === track) {
                        activeAudioTrack = null
                    }
                }
            }.apply {
                name = "astryx-tts-play"
                isDaemon = true
                start()
            }
            true
        } catch (_: Throwable) {
            isPlayingSpeech = false
            currentSpeakLevel = 0f
            false
        }
    }

    fun rustSpeakFallbackTts(text: String) {
        ensureTts()
        runOnUiThread {
            try {
                if (ttsReady) {
                    ttsEngine?.speak(text, TextToSpeech.QUEUE_FLUSH, null, "gemini_reply")
                }
            } catch (_: Throwable) {
            }
        }
    }

    fun rustStopSpeaking() {
        stopSpeechRequested = true
        isPlayingSpeech = false
        currentSpeakLevel = 0f
        try {
            activeAudioTrack?.pause()
            activeAudioTrack?.flush()
            activeAudioTrack?.stop()
        } catch (_: Throwable) {
        }
        try {
            ttsEngine?.stop()
        } catch (_: Throwable) {
        }
    }

    fun rustStopVoiceSession() {
        isRecordingVoice = false
        rustStopSpeaking()
    }

    fun rustIsSpeaking(): Boolean = isPlayingSpeech

    fun rustGetSpeakLevel(): Float {
        if (!isPlayingSpeech) return 0f
        // If using system TTS fallback where PCM isn't directly sampled, synthesize smooth speech cadence
        if (activeAudioTrack == null) {
            val t = System.currentTimeMillis() / 110.0
            return (0.35 + 0.35 * kotlin.math.sin(t) * kotlin.math.cos(t * 1.7)).toFloat().coerceIn(0.1f, 0.85f)
        }
        return currentSpeakLevel
    }
}
