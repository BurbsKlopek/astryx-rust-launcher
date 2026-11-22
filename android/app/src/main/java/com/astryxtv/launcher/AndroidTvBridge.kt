package com.astryxtv.launcher

import android.app.Activity
import android.app.ActivityManager
import android.app.UiModeManager
import android.app.role.RoleManager
import android.content.Context
import android.content.Intent
import android.content.pm.ApplicationInfo
import android.content.pm.PackageManager
import android.content.res.Configuration
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import android.net.Uri
import android.os.Build
import android.os.Environment
import android.os.StatFs
import android.provider.Settings
import android.webkit.JavascriptInterface
import org.json.JSONArray
import org.json.JSONObject

class AndroidTvBridge(private val context: Context) {

    private val prefs = context.getSharedPreferences("astryx_tv_prefs", Context.MODE_PRIVATE)
    @Volatile
    private var cachedAppsJson: String? = null

    fun invalidateAppsCache() {
        cachedAppsJson = null
    }

    @JavascriptInterface
    fun getInstalledApps(): String {
        cachedAppsJson?.let { return it }
        val pm = context.packageManager
        val selfPkg = context.packageName
        val seen = LinkedHashMap<String, JSONObject>()

        fun addFromIntent(intent: Intent, isLeanback: Boolean) {
            val resolved = pm.queryIntentActivities(intent, PackageManager.MATCH_ALL)
            for (ri in resolved) {
                val pkg = ri.activityInfo?.packageName ?: continue
                if (pkg == selfPkg) continue
                if (seen.containsKey(pkg)) {
                    if (isLeanback) {
                        seen[pkg]?.put("isLeanback", true)
                    }
                    continue
                }

                val appInfo = ri.activityInfo.applicationInfo
                val label = ri.loadLabel(pm)?.toString()?.trim().orEmpty().ifBlank { pkg }
                val hasBanner = try {
                    pm.getApplicationBanner(pkg) != null || ri.activityInfo.banner != 0
                } catch (_: Exception) {
                    false
                }
                val isSystem = (appInfo.flags and ApplicationInfo.FLAG_SYSTEM) != 0
                val category = inferCategory(pkg, label, appInfo)
                val versionName = try {
                    pm.getPackageInfo(pkg, 0).versionName ?: ""
                } catch (_: Exception) {
                    ""
                }

                val obj = JSONObject().apply {
                    put("packageName", pkg)
                    put("label", label)
                    put("category", category)
                    put("isLeanback", isLeanback)
                    put("hasBanner", hasBanner)
                    put("isSystemApp", isSystem)
                    put("versionName", versionName)
                    put("bannerUrl", "https://appassets.androidplatform.net/app-icon/banner/$pkg.png")
                    put("iconUrl", "https://appassets.androidplatform.net/app-icon/icon/$pkg.png")
                }
                seen[pkg] = obj
            }
        }

        // Leanback TV apps first
        addFromIntent(
            Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_LEANBACK_LAUNCHER),
            isLeanback = true
        )
        // Standard Android apps (sideloaded / mobile)
        addFromIntent(
            Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_LAUNCHER),
            isLeanback = false
        )

        val sorted = seen.values.sortedWith(
            compareByDescending<JSONObject> { it.optBoolean("isLeanback", false) }
                .thenBy { it.optString("label").lowercase() }
        )

        val result = JSONArray()
        for (item in sorted) {
            result.put(item)
        }
        val json = result.toString()
        cachedAppsJson = json
        return json
    }

    private fun inferCategory(pkg: String, label: String, info: ApplicationInfo): String {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            when (info.category) {
                ApplicationInfo.CATEGORY_VIDEO -> return "streaming"
                ApplicationInfo.CATEGORY_AUDIO -> return "music"
                ApplicationInfo.CATEGORY_GAME -> return "games"
                ApplicationInfo.CATEGORY_PRODUCTIVITY,
                ApplicationInfo.CATEGORY_NEWS -> return "tools"
            }
        }
        val hay = "$pkg $label".lowercase()
        return when {
            listOf("netflix", "youtube", "disney", "prime", "plex", "kodi", "jellyfin", "stremio", "iplayer", "itvx", "channel4", "vlc", "smarttube", "twitch", "hbo", "apple", "tv", "video", "cinema", "butter").any { it in hay } -> "streaming"
            listOf("spotify", "music", "tidal", "soundcloud", "radio", "deezer", "audio").any { it in hay } -> "music"
            listOf("game", "retro", "steam", "geforce", "moonlight", "xbox", "playstation", "emu").any { it in hay } -> "games"
            else -> "tools"
        }
    }

    @JavascriptInterface
    fun launchApp(packageName: String): Boolean {
        val pm = context.packageManager
        val intent = pm.getLeanbackLaunchIntentForPackage(packageName)
            ?: pm.getLaunchIntentForPackage(packageName)
            ?: return false
        intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        return try {
            context.startActivity(intent)
            true
        } catch (_: Exception) {
            false
        }
    }

    @JavascriptInterface
    fun openAppInfo(packageName: String) {
        try {
            val intent = Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS).apply {
                data = Uri.parse("package:$packageName")
                addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            }
            context.startActivity(intent)
        } catch (_: Exception) {
        }
    }

    @JavascriptInterface
    fun uninstallApp(packageName: String) {
        try {
            val intent = Intent(Intent.ACTION_DELETE).apply {
                data = Uri.parse("package:$packageName")
                addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            }
            context.startActivity(intent)
        } catch (_: Exception) {
        }
    }

    @JavascriptInterface
    fun isDefaultHomeLauncher(): Boolean {
        return try {
            val intent = Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_HOME)
            val resolved = context.packageManager.resolveActivity(intent, PackageManager.MATCH_DEFAULT_ONLY)
            resolved?.activityInfo?.packageName == context.packageName
        } catch (_: Exception) {
            false
        }
    }

    @JavascriptInterface
    fun openHomeLauncherSettings(): Boolean {
        // 1. Android 10+ (API 29+) RoleManager system prompt for ROLE_HOME
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q && context is Activity) {
            try {
                val rm = context.getSystemService(Context.ROLE_SERVICE) as RoleManager
                if (rm.isRoleAvailable(RoleManager.ROLE_HOME) && !rm.isRoleHeld(RoleManager.ROLE_HOME)) {
                    val roleIntent = rm.createRequestRoleIntent(RoleManager.ROLE_HOME)
                    context.runOnUiThread {
                        try {
                            @Suppress("DEPRECATION")
                            context.startActivityForResult(roleIntent, 9001)
                        } catch (_: Exception) {
                        }
                    }
                    return true
                }
            } catch (_: Exception) {
            }
        }

        // 2. Fallback to Android Home Settings / Default Apps Settings
        val fallbacks = listOf(
            Settings.ACTION_HOME_SETTINGS,
            Settings.ACTION_MANAGE_DEFAULT_APPS_SETTINGS,
            Settings.ACTION_SETTINGS
        )
        for (action in fallbacks) {
            try {
                val intent = Intent(action).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                if (intent.resolveActivity(context.packageManager) != null) {
                    context.startActivity(intent)
                    return true
                }
            } catch (_: Exception) {
            }
        }
        return false
    }

    @JavascriptInterface
    fun openSystemSettings(section: String) {
        val action = when (section.lowercase()) {
            "network", "wifi" -> Settings.ACTION_WIFI_SETTINGS
            "display" -> Settings.ACTION_DISPLAY_SETTINGS
            "apps" -> Settings.ACTION_APPLICATION_SETTINGS
            "date" -> Settings.ACTION_DATE_SETTINGS
            else -> Settings.ACTION_SETTINGS
        }
        try {
            val intent = Intent(action).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            context.startActivity(intent)
        } catch (_: Exception) {
            try {
                context.startActivity(
                    Intent(Settings.ACTION_SETTINGS).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                )
            } catch (_: Exception) {
            }
        }
    }

    @JavascriptInterface
    fun getSystemStatus(): String {
        val cm = context.getSystemService(Context.CONNECTIVITY_SERVICE) as ConnectivityManager
        val activeNetwork = cm.activeNetwork
        val caps = if (activeNetwork != null) cm.getNetworkCapabilities(activeNetwork) else null

        var isConnected = false
        var networkType = "none"
        var isVpnActive = false

        if (caps != null) {
            isConnected = caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET)
            isVpnActive = caps.hasTransport(NetworkCapabilities.TRANSPORT_VPN)
            networkType = when {
                caps.hasTransport(NetworkCapabilities.TRANSPORT_ETHERNET) -> "ethernet"
                caps.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) -> "wifi"
                caps.hasTransport(NetworkCapabilities.TRANSPORT_CELLULAR) -> "cellular"
                else -> if (isConnected) "wifi" else "none"
            }
        }

        // Also check all networks for VPN transport (Android TV often keeps VPN on a secondary network)
        if (!isVpnActive) {
            @Suppress("DEPRECATION")
            for (net in cm.allNetworks) {
                val netCaps = cm.getNetworkCapabilities(net)
                if (netCaps?.hasTransport(NetworkCapabilities.TRANSPORT_VPN) == true) {
                    isVpnActive = true
                    break
                }
            }
        }

        val uiMode = context.getSystemService(Context.UI_MODE_SERVICE) as UiModeManager
        val isTv = uiMode.currentModeType == Configuration.UI_MODE_TYPE_TELEVISION

        val am = context.getSystemService(Context.ACTIVITY_SERVICE) as ActivityManager
        val memInfo = ActivityManager.MemoryInfo()
        am.getMemoryInfo(memInfo)
        val ramFreeMb = (memInfo.availMem / (1024L * 1024L)).toInt()
        val ramTotalMb = (memInfo.totalMem / (1024L * 1024L)).toInt()

        var storageFreeGb = 0.0
        var storageTotalGb = 0.0
        try {
            val stat = StatFs(Environment.getDataDirectory().path)
            val bytesPerGb = 1024.0 * 1024.0 * 1024.0
            storageFreeGb = Math.round((stat.availableBytes / bytesPerGb) * 10.0) / 10.0
            storageTotalGb = Math.round((stat.totalBytes / bytesPerGb) * 10.0) / 10.0
        } catch (_: Exception) {
        }

        return JSONObject().apply {
            put("networkType", networkType)
            put("isConnected", isConnected)
            put("isVpnActive", isVpnActive)
            put("deviceModel", "${Build.MANUFACTURER} ${Build.MODEL}".trim())
            put("androidVersion", "Android ${Build.VERSION.RELEASE}")
            put("isTv", isTv)
            put("ramFreeMb", ramFreeMb)
            put("ramTotalMb", ramTotalMb)
            put("storageFreeGb", storageFreeGb)
            put("storageTotalGb", storageTotalGb)
        }.toString()
    }

    @JavascriptInterface
    fun getPreference(key: String, defaultValue: String): String {
        return prefs.getString(key, defaultValue) ?: defaultValue
    }

    @JavascriptInterface
    fun setPreference(key: String, value: String) {
        prefs.edit().putString(key, value).apply()
    }
}
