package com.astryxtv.rustlauncher

import android.app.ActivityManager
import android.app.NativeActivity
import android.app.PendingIntent
import android.app.role.RoleManager
import android.content.Context
import android.content.Intent
import android.content.pm.ApplicationInfo
import android.content.pm.PackageInstaller
import android.content.pm.PackageManager
import java.io.File
import java.io.FileInputStream
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.drawable.BitmapDrawable
import android.graphics.drawable.Drawable
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
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

    private val prefs by lazy {
        getSharedPreferences("astryx_tv_rust_prefs", Context.MODE_PRIVATE)
    }

    private external fun nativeOnBackPressed()
    private external fun nativeOnMenuPressed()

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
    }

    override fun surfaceCreated(holder: SurfaceHolder) {
        super.surfaceCreated(holder)
        applySurfaceFrameRate(holder)
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
        if (hasFocus) enterImmersiveMode()
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
        }
    }

    fun rustOpenSystemSettings(section: String) {
        val action = when (section) {
            "network" -> Settings.ACTION_WIFI_SETTINGS
            "apps" -> Settings.ACTION_APPLICATION_SETTINGS
            "display" -> Settings.ACTION_DISPLAY_SETTINGS
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
}
