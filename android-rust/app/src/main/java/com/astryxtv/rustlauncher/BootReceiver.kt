package com.astryxtv.rustlauncher

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent

/**
 * Ensures Astryx Rust Launcher starts cleanly on TV cold-boot so configured
 * Startup Options (open chosen app, resume last app, or open TV/HDMI input) run automatically.
 */
class BootReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action == Intent.ACTION_BOOT_COMPLETED) {
            try {
                val launch = Intent(context, RustLauncherActivity::class.java).apply {
                    addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                    putExtra("astryx_from_boot", true)
                }
                context.startActivity(launch)
            } catch (_: Throwable) {
            }
        }
    }
}
