package com.astryxtv.launcher

import android.content.Context
import android.content.pm.PackageManager
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.drawable.BitmapDrawable
import android.graphics.drawable.Drawable
import android.os.Build
import android.util.LruCache
import android.webkit.WebResourceResponse
import androidx.webkit.WebViewAssetLoader
import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream

/**
 * Serves Android TV banners and app icons as cached PNG streams via WebViewAssetLoader:
 *   https://appassets.androidplatform.net/app-icon/banner/<packageName>.png
 *   https://appassets.androidplatform.net/app-icon/icon/<packageName>.png
 */
class AppIconPathHandler(private val context: Context) : WebViewAssetLoader.PathHandler {

    private val cache = object : LruCache<String, ByteArray>(16 * 1024 * 1024) {
        override fun sizeOf(key: String, value: ByteArray): Int = value.size
    }

    fun clearCache() {
        cache.evictAll()
    }

    override fun handle(path: String): WebResourceResponse? {
        val clean = path.removePrefix("/")
        val parts = clean.split("/", limit = 2)
        if (parts.size != 2) return null

        val kind = parts[0] // "banner" or "icon"
        val packageName = parts[1].removeSuffix(".png")
        if (packageName.isBlank()) return null

        val cacheKey = "$kind:$packageName"
        val cached = cache.get(cacheKey)
        if (cached != null) {
            return pngResponse(cached)
        }

        val pm = context.packageManager
        val drawable: Drawable? = try {
            if (kind == "banner") {
                pm.getApplicationBanner(packageName) ?: pm.getApplicationIcon(packageName)
            } else {
                pm.getApplicationIcon(packageName)
            }
        } catch (_: PackageManager.NameNotFoundException) {
            null
        } catch (_: Exception) {
            null
        }

        if (drawable == null) return null

        val isBanner = kind == "banner" && try {
            pm.getApplicationBanner(packageName) != null
        } catch (_: Exception) {
            false
        }

        val targetW = if (isBanner) 240 else 144
        val targetH = if (isBanner) 135 else 144
        val pngBytes = drawableToPng(drawable, targetW, targetH)
        cache.put(cacheKey, pngBytes)
        return pngResponse(pngBytes)
    }

    private fun drawableToPng(drawable: Drawable, width: Int, height: Int): ByteArray {
        val bitmap = if (drawable is BitmapDrawable && drawable.bitmap != null &&
            drawable.bitmap.width <= width * 2 && drawable.bitmap.height <= height * 2
        ) {
            drawable.bitmap
        } else {
            val w = drawable.intrinsicWidth.takeIf { it > 0 }?.coerceAtMost(width) ?: width
            val h = drawable.intrinsicHeight.takeIf { it > 0 }?.coerceAtMost(height) ?: height
            Bitmap.createBitmap(w, h, Bitmap.Config.ARGB_8888).also { bmp ->
                val canvas = Canvas(bmp)
                drawable.setBounds(0, 0, canvas.width, canvas.height)
                drawable.draw(canvas)
            }
        }

        val out = ByteArrayOutputStream()
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            bitmap.compress(Bitmap.CompressFormat.WEBP_LOSSY, 84, out)
        } else {
            @Suppress("DEPRECATION")
            bitmap.compress(Bitmap.CompressFormat.WEBP, 84, out)
        }
        return out.toByteArray()
    }

    private fun pngResponse(bytes: ByteArray): WebResourceResponse {
        val headers = mapOf(
            "Cache-Control" to "public, max-age=86400",
            "Access-Control-Allow-Origin" to "*"
        )
        return WebResourceResponse("image/webp", null, 200, "OK", headers, ByteArrayInputStream(bytes))
    }
}
