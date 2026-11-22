package com.astryxtv.launcher

import android.annotation.SuppressLint
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.graphics.Color
import android.os.Build
import android.os.Bundle
import android.view.KeyEvent
import android.view.View
import android.view.WindowInsets
import android.view.WindowInsetsController
import android.webkit.WebChromeClient
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebSettings
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.appcompat.app.AppCompatActivity
import androidx.webkit.WebViewAssetLoader

class MainActivity : AppCompatActivity() {

    private lateinit var webView: WebView
    private lateinit var iconPathHandler: AppIconPathHandler
    private lateinit var tvBridge: AndroidTvBridge
    private var centerDownAt: Long = 0L
    private var longPressDispatched = false

    private val packageReceiver = object : BroadcastReceiver() {
        override fun onReceive(context: Context?, intent: Intent?) {
            iconPathHandler.clearCache()
            tvBridge.invalidateAppsCache()
            dispatchJs("window.__tvPackagesChanged && window.__tvPackagesChanged();")
        }
    }

    @SuppressLint("SetJavaScriptEnabled")
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        iconPathHandler = AppIconPathHandler(this)
        tvBridge = AndroidTvBridge(this)

        // Pre-warm installed apps cache off the UI thread while WebView initializes
        java.util.concurrent.Executors.newSingleThreadExecutor().execute {
            tvBridge.getInstalledApps()
        }

        val assetLoader = WebViewAssetLoader.Builder()
            .addPathHandler("/assets/", WebViewAssetLoader.AssetsPathHandler(this))
            .addPathHandler("/app-icon/", iconPathHandler)
            .build()

        webView = WebView(this).apply {
            setBackgroundColor(Color.parseColor("#06070c"))
            isFocusable = true
            isFocusableInTouchMode = true
            overScrollMode = View.OVER_SCROLL_NEVER

            settings.apply {
                javaScriptEnabled = true
                domStorageEnabled = true
                mediaPlaybackRequiresUserGesture = false
                allowFileAccess = false
                allowContentAccess = false
                mixedContentMode = WebSettings.MIXED_CONTENT_COMPATIBILITY_MODE
                cacheMode = WebSettings.LOAD_DEFAULT
                useWideViewPort = true
                loadWithOverviewMode = true
            }

            addJavascriptInterface(tvBridge, "AndroidTv")

            webChromeClient = object : WebChromeClient() {
                override fun onConsoleMessage(cm: android.webkit.ConsoleMessage): Boolean {
                    android.util.Log.d(
                        "AstryxTVWeb",
                        "[${cm.messageLevel()}] ${cm.message()} (${cm.sourceId()}:${cm.lineNumber()})"
                    )
                    return true
                }
            }
            webViewClient = object : WebViewClient() {
                override fun shouldInterceptRequest(
                    view: WebView,
                    request: WebResourceRequest
                ): WebResourceResponse? {
                    return assetLoader.shouldInterceptRequest(request.url)
                }
            }
        }

        setContentView(webView)
        enterImmersiveMode()

        webView.loadUrl("https://appassets.androidplatform.net/assets/www/index.html")
        webView.requestFocus()

        val filter = IntentFilter().apply {
            addAction(Intent.ACTION_PACKAGE_ADDED)
            addAction(Intent.ACTION_PACKAGE_REMOVED)
            addAction(Intent.ACTION_PACKAGE_CHANGED)
            addDataScheme("package")
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            registerReceiver(packageReceiver, filter, RECEIVER_NOT_EXPORTED)
        } else {
            registerReceiver(packageReceiver, filter)
        }
    }

    override fun onStart() {
        super.onStart()
        webView.onResume()
        webView.resumeTimers()
    }

    override fun onResume() {
        super.onResume()
        enterImmersiveMode()
        dispatchJs("window.__tvResume && window.__tvResume();")
    }

    override fun onStop() {
        webView.pauseTimers()
        webView.onPause()
        super.onStop()
    }

    override fun onDestroy() {
        try {
            unregisterReceiver(packageReceiver)
        } catch (_: Exception) {
        }
        webView.destroy()
        super.onDestroy()
    }

    override fun dispatchKeyEvent(event: KeyEvent): Boolean {
        val code = event.keyCode
        val isNavKey = code == KeyEvent.KEYCODE_DPAD_UP ||
            code == KeyEvent.KEYCODE_DPAD_DOWN ||
            code == KeyEvent.KEYCODE_DPAD_LEFT ||
            code == KeyEvent.KEYCODE_DPAD_RIGHT ||
            code == KeyEvent.KEYCODE_DPAD_CENTER ||
            code == KeyEvent.KEYCODE_ENTER ||
            code == KeyEvent.KEYCODE_BACK ||
            code == KeyEvent.KEYCODE_MENU ||
            code == KeyEvent.KEYCODE_INFO

        if (!isNavKey) {
            return super.dispatchKeyEvent(event)
        }

        // Allow native text input when an <input> or <textarea> is focused inside WebView
        if (code == KeyEvent.KEYCODE_DPAD_CENTER || code == KeyEvent.KEYCODE_ENTER) {
            if (event.action == KeyEvent.ACTION_DOWN) {
                if (event.repeatCount == 0) {
                    centerDownAt = System.currentTimeMillis()
                    longPressDispatched = false
                } else if (!longPressDispatched && System.currentTimeMillis() - centerDownAt > 520) {
                    longPressDispatched = true
                    dispatchJs("window.__tvKeyDown && window.__tvKeyDown('SelectLong');")
                }
                return true
            } else if (event.action == KeyEvent.ACTION_UP) {
                if (!longPressDispatched) {
                    dispatchJs("window.__tvKeyDown && window.__tvKeyDown('Select');")
                }
                longPressDispatched = false
                return true
            }
        }

        if (event.action == KeyEvent.ACTION_DOWN) {
            val mapped = when (code) {
                KeyEvent.KEYCODE_DPAD_UP -> "ArrowUp"
                KeyEvent.KEYCODE_DPAD_DOWN -> "ArrowDown"
                KeyEvent.KEYCODE_DPAD_LEFT -> "ArrowLeft"
                KeyEvent.KEYCODE_DPAD_RIGHT -> "ArrowRight"
                KeyEvent.KEYCODE_BACK -> "Back"
                KeyEvent.KEYCODE_MENU, KeyEvent.KEYCODE_INFO -> "Menu"
                else -> null
            }
            if (mapped != null) {
                dispatchJs("window.__tvKeyDown && window.__tvKeyDown('$mapped');")
                return true
            }
        } else if (event.action == KeyEvent.ACTION_UP) {
            return true
        }

        return super.dispatchKeyEvent(event)
    }

    private fun dispatchJs(script: String) {
        runOnUiThread {
            webView.evaluateJavascript(script, null)
        }
    }

    private fun enterImmersiveMode() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            window.setDecorFitsSystemWindows(false)
            window.insetsController?.let { controller ->
                controller.hide(WindowInsets.Type.statusBars() or WindowInsets.Type.navigationBars())
                controller.systemBarsBehavior =
                    WindowInsetsController.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
            }
        } else {
            @Suppress("DEPRECATION")
            window.decorView.systemUiVisibility = (
                View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY or
                    View.SYSTEM_UI_FLAG_FULLSCREEN or
                    View.SYSTEM_UI_FLAG_HIDE_NAVIGATION or
                    View.SYSTEM_UI_FLAG_LAYOUT_STABLE or
                    View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN or
                    View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION
                )
        }
    }
}
