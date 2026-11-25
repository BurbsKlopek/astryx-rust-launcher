use egui::ColorImage;
use jni::objects::{JByteArray, JObject, JString, JValue};
use jni::JavaVM;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TvAppInfo {
    pub package_name: String,
    pub label: String,
    pub category: String,
    pub is_leanback: bool,
    pub is_system_app: bool,
    pub version_name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TvSystemStatus {
    pub network_type: String,
    pub is_connected: bool,
    pub is_vpn_active: bool,
    pub device_model: String,
    pub android_version: String,
    pub ram_free_mb: i32,
    pub ram_total_mb: i32,
    pub storage_free_gb: f64,
    pub storage_total_gb: f64,
}

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

static ANDROID_HANDLES: OnceLock<(usize, usize)> = OnceLock::new();
static EGUI_CTX: OnceLock<egui::Context> = OnceLock::new();
static BACK_PRESSED: AtomicBool = AtomicBool::new(false);
static MENU_PRESSED: AtomicBool = AtomicBool::new(false);
static STATUS_REFRESH_REQUESTED: AtomicBool = AtomicBool::new(false);
static PACKAGES_CHANGED: AtomicBool = AtomicBool::new(false);

pub fn init(vm_ptr: *mut c_void, activity_ptr: *mut c_void) {
    let _ = ANDROID_HANDLES.set((vm_ptr as usize, activity_ptr as usize));
}

pub fn set_egui_ctx(ctx: egui::Context) {
    let _ = EGUI_CTX.set(ctx);
}

pub fn take_back_pressed() -> bool {
    BACK_PRESSED.swap(false, Ordering::SeqCst)
}

pub fn take_menu_pressed() -> bool {
    MENU_PRESSED.swap(false, Ordering::SeqCst)
}

pub fn take_status_refresh_requested() -> bool {
    STATUS_REFRESH_REQUESTED.swap(false, Ordering::SeqCst)
}

pub fn take_packages_changed() -> bool {
    PACKAGES_CHANGED.swap(false, Ordering::SeqCst)
}

#[no_mangle]
pub extern "system" fn Java_com_astryxtv_rustlauncher_RustLauncherActivity_nativeOnBackPressed(
    _env: jni::JNIEnv,
    _obj: JObject,
) {
    BACK_PRESSED.store(true, Ordering::SeqCst);
    if let Some(ctx) = EGUI_CTX.get() {
        ctx.request_repaint();
    }
}

#[no_mangle]
pub extern "system" fn Java_com_astryxtv_rustlauncher_RustLauncherActivity_nativeOnMenuPressed(
    _env: jni::JNIEnv,
    _obj: JObject,
) {
    MENU_PRESSED.store(true, Ordering::SeqCst);
    if let Some(ctx) = EGUI_CTX.get() {
        ctx.request_repaint();
    }
}

#[no_mangle]
pub extern "system" fn Java_com_astryxtv_rustlauncher_RustLauncherActivity_nativeOnStatusChanged(
    _env: jni::JNIEnv,
    _obj: JObject,
) {
    STATUS_REFRESH_REQUESTED.store(true, Ordering::SeqCst);
    if let Some(ctx) = EGUI_CTX.get() {
        ctx.request_repaint();
    }
}

#[no_mangle]
pub extern "system" fn Java_com_astryxtv_rustlauncher_RustLauncherActivity_nativeOnPackagesChanged(
    _env: jni::JNIEnv,
    _obj: JObject,
) {
    PACKAGES_CHANGED.store(true, Ordering::SeqCst);
    if let Some(ctx) = EGUI_CTX.get() {
        ctx.request_repaint();
    }
}

fn with_activity<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&mut jni::JNIEnv, &JObject) -> Option<R>,
{
    let (vm_ptr, act_ptr) = match ANDROID_HANDLES.get().copied() {
        Some((v, a)) if v != 0 && a != 0 => (v as *mut c_void, a as *mut c_void),
        _ => {
            let ctx = ndk_context::android_context();
            (ctx.vm(), ctx.context())
        }
    };
    let vm = unsafe { JavaVM::from_raw(vm_ptr.cast()).ok()? };
    let mut env = vm.attach_current_thread().ok()?;
    let activity = unsafe { JObject::from_raw(act_ptr.cast()) };
    let result = f(&mut env, &activity);
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_describe();
        let _ = env.exception_clear();
    }
    // Do not drop borrowed global activity ref
    std::mem::forget(activity);
    result
}

pub fn get_installed_apps() -> Vec<TvAppInfo> {
    with_activity(|env, act| {
        let val = env
            .call_method(act, "rustGetInstalledApps", "()Ljava/lang/String;", &[])
            .ok()?
            .l()
            .ok()?;
        let s: String = env.get_string(&JString::from(val)).ok()?.into();
        serde_json::from_str(&s).ok()
    })
    .unwrap_or_default()
}

pub fn get_app_icon_circular(pkg: &str) -> Option<ColorImage> {
    let mut rgba_bytes = with_activity(|env, act| {
        let jpkg = env.new_string(pkg).ok()?;
        let obj = env
            .call_method(
                act,
                "rustGetAppIconRgba",
                "(Ljava/lang/String;)[B",
                &[JValue::Object(&jpkg)],
            )
            .ok()?
            .l()
            .ok()?;
        if obj.is_null() {
            return None;
        }
        let arr = JByteArray::from(obj);
        env.convert_byte_array(arr).ok()
    })?;

    const SIZE: usize = 256;
    if rgba_bytes.len() != SIZE * SIZE * 4 {
        return None;
    }

    let cx = SIZE as f32 * 0.5;
    let cy = SIZE as f32 * 0.5;
    let r = SIZE as f32 * 0.5;

    // Fast 2px anti-aliased circular alpha mask directly on raw RGBA buffer (zero PNG decode!)
    for y in 0..SIZE {
        let dy = y as f32 + 0.5 - cy;
        let dy2 = dy * dy;
        let row = y * SIZE * 4;
        for x in 0..SIZE {
            let dx = x as f32 + 0.5 - cx;
            let dist = (dx * dx + dy2).sqrt();
            let a_idx = row + x * 4 + 3;
            if dist >= r {
                rgba_bytes[a_idx] = 0;
            } else if dist > r - 2.0 {
                let factor = ((r - dist) * 0.5).clamp(0.0, 1.0);
                rgba_bytes[a_idx] = (rgba_bytes[a_idx] as f32 * factor) as u8;
            }
        }
    }

    Some(ColorImage::from_rgba_unmultiplied([SIZE, SIZE], &rgba_bytes))
}

pub fn launch_app(pkg: &str) -> bool {
    with_activity(|env, act| {
        let jpkg = env.new_string(pkg).ok()?;
        env.call_method(
            act,
            "rustLaunchApp",
            "(Ljava/lang/String;)Z",
            &[JValue::Object(&jpkg)],
        )
        .ok()?
        .z()
        .ok()
    })
    .unwrap_or(false)
}

pub fn open_app_info(pkg: &str) {
    let _ = with_activity(|env, act| {
        let jpkg = env.new_string(pkg).ok()?;
        env.call_method(
            act,
            "rustOpenAppInfo",
            "(Ljava/lang/String;)V",
            &[JValue::Object(&jpkg)],
        )
        .ok()?;
        Some(())
    });
}

pub fn open_url(url: &str) -> bool {
    with_activity(|env, act| {
        let jurl = env.new_string(url).ok()?;
        env.call_method(
            act,
            "rustOpenUrl",
            "(Ljava/lang/String;)Z",
            &[JValue::Object(&jurl)],
        )
        .ok()?
        .z()
        .ok()
    })
    .unwrap_or(false)
}

pub fn uninstall_app(pkg: &str) {
    let _ = with_activity(|env, act| {
        let jpkg = env.new_string(pkg).ok()?;
        env.call_method(
            act,
            "rustUninstallApp",
            "(Ljava/lang/String;)V",
            &[JValue::Object(&jpkg)],
        )
        .ok()?;
        Some(())
    });
}

pub fn open_system_settings(section: &str) {
    let _ = with_activity(|env, act| {
        let jsec = env.new_string(section).ok()?;
        env.call_method(
            act,
            "rustOpenSystemSettings",
            "(Ljava/lang/String;)V",
            &[JValue::Object(&jsec)],
        )
        .ok()?;
        Some(())
    });
}

pub fn is_default_home() -> bool {
    with_activity(|env, act| {
        env.call_method(act, "rustIsDefaultHome", "()Z", &[])
            .ok()?
            .z()
            .ok()
    })
    .unwrap_or(false)
}

pub fn open_home_settings() -> bool {
    with_activity(|env, act| {
        env.call_method(act, "rustOpenHomeSettings", "()Z", &[])
            .ok()?
            .z()
            .ok()
    })
    .unwrap_or(false)
}

pub fn get_system_status() -> TvSystemStatus {
    with_activity(|env, act| {
        let val = env
            .call_method(act, "rustGetSystemStatus", "()Ljava/lang/String;", &[])
            .ok()?
            .l()
            .ok()?;
        let s: String = env.get_string(&JString::from(val)).ok()?.into();
        serde_json::from_str(&s).ok()
    })
    .unwrap_or_default()
}

pub fn clear_ram() -> i32 {
    with_activity(|env, act| {
        env.call_method(act, "rustClearRam", "()I", &[])
            .ok()?
            .i()
            .ok()
    })
    .unwrap_or(0)
}

pub fn get_pref(key: &str) -> String {
    with_activity(|env, act| {
        let jkey = env.new_string(key).ok()?;
        let val = env
            .call_method(
                act,
                "rustGetPref",
                "(Ljava/lang/String;)Ljava/lang/String;",
                &[JValue::Object(&jkey)],
            )
            .ok()?
            .l()
            .ok()?;
        let s: String = env.get_string(&JString::from(val)).ok()?.into();
        Some(s)
    })
    .unwrap_or_default()
}

pub fn set_pref(key: &str, value: &str) {
    let _ = with_activity(|env, act| {
        let jkey = env.new_string(key).ok()?;
        let jval = env.new_string(value).ok()?;
        env.call_method(
            act,
            "rustSetPref",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            &[JValue::Object(&jkey), JValue::Object(&jval)],
        )
        .ok()?;
        Some(())
    });
}

pub fn get_app_version() -> String {
    with_activity(|env, act| {
        let val = env
            .call_method(act, "rustGetAppVersion", "()Ljava/lang/String;", &[])
            .ok()?
            .l()
            .ok()?;
        let s: String = env.get_string(&JString::from(val)).ok()?.into();
        Some(s)
    })
    .filter(|s| !s.is_empty())
    .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string())
}

pub fn get_refresh_rate() -> f32 {
    with_activity(|env, act| {
        env.call_method(act, "rustGetRefreshRate", "()F", &[])
            .ok()?
            .f()
            .ok()
    })
    .filter(|hz| *hz >= 50.0 && *hz <= 240.0)
    .unwrap_or(60.0)
}

pub fn set_fps_overlay(enabled: bool, flags: i32) -> String {
    with_activity(|env, act| {
        let val = env
            .call_method(
                act,
                "rustSetFpsOverlay",
                "(ZI)Ljava/lang/String;",
                &[JValue::Bool(u8::from(enabled)), JValue::Int(flags)],
            )
            .ok()?
            .l()
            .ok()?;
        let s: String = env.get_string(&JString::from(val)).ok()?.into();
        Some(s)
    })
    .unwrap_or_else(|| "ERR:JNI".to_string())
}

pub fn get_local_ip() -> String {
    with_activity(|env, act| {
        let val = env
            .call_method(act, "rustGetLocalIp", "()Ljava/lang/String;", &[])
            .ok()?
            .l()
            .ok()?;
        let s: String = env.get_string(&JString::from(val)).ok()?.into();
        Some(s)
    })
    .filter(|s| !s.is_empty())
    .unwrap_or_else(|| "127.0.0.1".to_string())
}

pub fn is_music_active() -> bool {
    with_activity(|env, act| {
        env.call_method(act, "rustIsMusicActive", "()Z", &[])
            .ok()?
            .z()
            .ok()
    })
    .unwrap_or(false)
}

pub fn media_play_pause() {
    let _ = with_activity(|env, act| {
        env.call_method(act, "rustMediaPlayPause", "()V", &[]).ok()?;
        Some(())
    });
}

pub fn trigger_sleep() {
    let _ = with_activity(|env, act| {
        env.call_method(act, "rustTriggerSleep", "()V", &[]).ok()?;
        Some(())
    });
}

pub fn open_tv_inputs() -> bool {
    with_activity(|env, act| {
        env.call_method(act, "rustOpenTvInputs", "()Z", &[])
            .ok()?
            .z()
            .ok()
    })
    .unwrap_or(false)
}

pub fn export_backup(json: &str) -> String {
    with_activity(|env, act| {
        let j = env.new_string(json).ok()?;
        let val = env
            .call_method(
                act,
                "rustExportBackup",
                "(Ljava/lang/String;)Ljava/lang/String;",
                &[JValue::Object(&j)],
            )
            .ok()?
            .l()
            .ok()?;
        let s: String = env.get_string(&JString::from(val)).ok()?.into();
        Some(s)
    })
    .unwrap_or_else(|| "ERR:JNI".to_string())
}

pub fn import_backup() -> String {
    with_activity(|env, act| {
        let val = env
            .call_method(act, "rustImportBackup", "()Ljava/lang/String;", &[])
            .ok()?
            .l()
            .ok()?;
        let s: String = env.get_string(&JString::from(val)).ok()?.into();
        Some(s)
    })
    .unwrap_or_default()
}

pub fn get_cache_dir() -> String {
    with_activity(|env, act| {
        let val = env
            .call_method(act, "rustGetCacheDir", "()Ljava/lang/String;", &[])
            .ok()?
            .l()
            .ok()?;
        let s: String = env.get_string(&JString::from(val)).ok()?.into();
        Some(s)
    })
    .filter(|s| !s.is_empty())
    .unwrap_or_else(|| "/data/user/0/com.astryxtv.rustlauncher/cache".to_string())
}

pub fn install_apk(apk_path: &str) -> String {
    with_activity(|env, act| {
        let jpath = env.new_string(apk_path).ok()?;
        let val = env
            .call_method(
                act,
                "rustInstallApk",
                "(Ljava/lang/String;)Ljava/lang/String;",
                &[JValue::Object(&jpath)],
            )
            .ok()?
            .l()
            .ok()?;
        let s: String = env.get_string(&JString::from(val)).ok()?.into();
        Some(s)
    })
    .unwrap_or_else(|| "ERR:Android JNI bridge unavailable".to_string())
}
