use crate::a2ui::{A2uiEngine, SAMPLE_A2UI_PAYLOAD};
use crate::bridge::{self, TvAppInfo, TvSystemStatus};
use crate::gemini;
use crate::theme::{
    AppFolder, AstryxThemeId, FocusAnimStyle, IconSize, LauncherConfig, LayoutStyle,
    ScreensaverStyle, SettingsViewMode, SortOrder, StartupMode, WeatherScene, ACCENT_PRESETS,
    WEATHER_CITY_PRESETS,
};
use crate::updater::{self, ReleaseInfo, UpdateCheckOutcome};
use crate::weather::{self, WeatherData};
use crate::web_server::{self, WebCmd, WEB_COMPANION_PORT};
use chrono::{Local, Timelike};
use egui::{
    Align, Align2, Color32, ColorImage, CornerRadius, FontId, Key, Pos2, Rect, RichText,
    ScrollArea, Sense, Stroke, TextureHandle, TextureOptions, Ui, Vec2,
};
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const PREF_CONFIG_KEY: &str = "astryx_rust_config_v1";
const DONATE_URL: &str = "https://buymeacoffee.com/burbsklopek";

enum BgMsg {
    AppsLoaded(Vec<TvAppInfo>),
    IconLoaded(String, ColorImage),
    StatusLoaded(TvSystemStatus, bool, bool), // (status, is_home, is_music_active)
    WeatherLoaded(WeatherData),
    CityGeocoded(String, f64, f64),
    WallpaperLoaded(WeatherScene, u8, ColorImage),
    RamCleared(i32, i32), // (before_mb, after_mb)
    UpdateChecked(UpdateCheckOutcome, bool), // (outcome, silent)
    UpdateDownloadProgress(u64, u64),
    UpdateDownloadDone(Result<String, String>),
    GeminiReplied(Result<gemini::GeminiReply, String>),
    GeminiA2uiReady(Result<String, String>),
}

#[derive(Debug, Clone)]
enum UpdaterState {
    Idle,
    Checking,
    UpToDate { latest: Option<String> },
    Available(ReleaseInfo),
    Downloading {
        release: ReleaseInfo,
        downloaded: u64,
        total: u64,
    },
    ReadyToInstall {
        release: ReleaseInfo,
        apk_path: String,
    },
    NeedPermission {
        release: ReleaseInfo,
        apk_path: String,
    },
    Error(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum FocusTarget {
    Dock(usize), // 0:Wifi, 1:Vpn, 2:Search, 3:Theme, 4:A2UI, 5:Refresh, 6:Settings
    Weather,
    Fav(usize),
    App(usize),
    RamPill,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ActiveModal {
    None,
    Welcome,
    Weather,
    Theme,
    Settings,
    Updater,
    Overlay,
    Donate,
    WeatherCity,
    Folders,
    PowerMenu,
    PhoneCompanion,
    Backup,
    StartupOptions,
    Search,
    Gemini,
    GeminiSettings,
    A2ui,
    AppContext(String), // package_name
    RenameApp(String),  // package_name
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettingsZone {
    Ribbon,   // Top Cinema Ribbon: 0..5 category tabs, 5 = Close button
    List,     // Left 60% settings list
    QuickBar, // Bottom-right Android TV shortcuts (0..4) & Boost RAM (4)
}

pub struct AstryxTvApp {
    config: LauncherConfig,
    apps: Vec<TvAppInfo>,
    cached_visible: Arc<Vec<TvAppInfo>>,
    cached_favs: Arc<Vec<TvAppInfo>>,
    cached_sec: u32,
    cached_time_str: String,
    cached_date_str: String,
    cached_date_short: String,
    status: TvSystemStatus,
    is_default_home: bool,
    is_music_active: bool,
    local_ip: String,
    weather: Option<WeatherData>,
    use_fahrenheit: bool,

    // Textures
    icon_textures: HashMap<String, TextureHandle>,
    icon_loading: HashSet<String>,
    wallpaper_tex: Option<(WeatherScene, u8, TextureHandle)>,
    wallpaper_loading: Option<(WeatherScene, u8)>,

    // A2UI & Gemini AI (Voice-First by default)
    a2ui: A2uiEngine,
    a2ui_input: String,
    a2ui_feedback: String,
    gemini_prompt: String,
    gemini_reply: String,
    gemini_last_id: Option<String>,
    gemini_busy: bool,
    gemini_text_mode: bool,
    gemini_listening: bool,

    // Internal OTA Updater
    app_version: String,
    display_hz: f32,
    fps_frame_count: u32,
    fps_window_start: Instant,
    fps_current: f32,
    fps_overlay_active: bool,
    updater_state: UpdaterState,

    // Navigation & Modals
    focus: FocusTarget,
    last_app_idx: usize,
    focus_changed: bool,
    focus_changed_at: Instant,
    modal: ActiveModal,
    modal_stack: Vec<(ActiveModal, usize)>,
    modal_focus: usize,
    modal_close_focused: bool,
    modal_just_opened: bool,
    modal_wait_key_release: bool,
    modal_focus_changed: bool,
    modal_opened_at: Instant,
    last_back_handled: Instant,
    last_sat_refresh: Instant,
    settings_tab: usize,
    settings_zone: SettingsZone,
    settings_ribbon_focus: usize,
    settings_sys_col: usize,
    select_down_since: Option<Instant>,
    select_long_triggered: bool,
    search_query: String,
    rename_input: String,
    toast: Option<(String, Instant)>,

    // Async messaging & timers
    tx: Sender<BgMsg>,
    rx: Receiver<BgMsg>,
    web_rx: Receiver<WebCmd>,
    config_snapshot: Arc<Mutex<String>>,
    last_input_at: Instant,
    ambient_active: bool,
    active_folder_idx: Option<usize>,
    startup_pending: bool,
    startup_started_at: Instant,
    start_time: Instant,
    last_status_poll: Instant,
    last_weather_poll: Instant,
    last_grid_cols: usize,
    pending_apps_refresh: bool,
}

impl AstryxTvApp {
    fn install_crisp_tv_fonts(ctx: &egui::Context) {
        let candidates = [
            "/system/fonts/Google_Sans-500-100_0-0_0.ttf",
            "/system/fonts/DroidSans-Bold.ttf",
            "/system/fonts/RobotoStatic-Regular.ttf",
        ];
        for path in candidates {
            if let Ok(bytes) = std::fs::read(path) {
                let mut fonts = egui::FontDefinitions::default();
                fonts.font_data.insert(
                    "tv_sans".to_owned(),
                    std::sync::Arc::new(egui::FontData::from_owned(bytes)),
                );
                if let Some(fam) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
                    fam.insert(0, "tv_sans".to_owned());
                }
                ctx.set_fonts(fonts);
                log::info!("Loaded crisp Android TV system font: {path}");
                break;
            }
        }
    }

    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        bridge::set_egui_ctx(cc.egui_ctx.clone());
        Self::install_crisp_tv_fonts(&cc.egui_ctx);

        // Smooth 4K vector tessellation (0.2 tolerance = visually perfect circles at 3x lower CPU cost)
        cc.egui_ctx.options_mut(|o| {
            o.tessellation_options.feathering = true;
            o.tessellation_options.feathering_size_in_pixels = 1.0;
            o.tessellation_options.bezier_tolerance = 0.2;
            o.tessellation_options.round_text_to_pixels = true;
        });

        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = Color32::TRANSPARENT;
        visuals.window_fill = Color32::from_rgba_unmultiplied(14, 16, 28, 220);
        cc.egui_ctx.set_visuals(visuals);

        // Load saved config
        let saved = bridge::get_pref(PREF_CONFIG_KEY);
        let config: LauncherConfig = if !saved.is_empty() {
            serde_json::from_str(&saved).unwrap_or_default()
        } else {
            LauncherConfig::default()
        };

        let (tx, rx) = mpsc::channel();
        let (web_tx, web_rx) = mpsc::channel();
        let now = Instant::now();
        let config_json = serde_json::to_string_pretty(&config).unwrap_or_else(|_| "{}".to_string());
        let config_snapshot = Arc::new(Mutex::new(config_json));
        let local_ip = bridge::get_local_ip();
        let web_enabled = config.web_companion_enabled;
        let show_welcome = !config.onboarding_done;
        let startup_pending = config.onboarding_done && config.startup_mode != StartupMode::Home;
        if config.lightweight_mode {
            cc.egui_ctx.options_mut(|o| {
                o.tessellation_options.bezier_tolerance = 0.6;
            });
        }

        let mut app = Self {
            config,
            apps: Vec::new(),
            cached_visible: Arc::new(Vec::new()),
            cached_favs: Arc::new(Vec::new()),
            cached_sec: u32::MAX,
            cached_time_str: String::new(),
            cached_date_str: String::new(),
            cached_date_short: String::new(),
            status: TvSystemStatus::default(),
            is_default_home: false,
            is_music_active: false,
            local_ip,
            weather: None,
            use_fahrenheit: false,
            icon_textures: HashMap::new(),
            icon_loading: HashSet::new(),
            wallpaper_tex: None,
            wallpaper_loading: None,
            a2ui: A2uiEngine::default(),
            a2ui_input: SAMPLE_A2UI_PAYLOAD.to_string(),
            a2ui_feedback: String::new(),
            gemini_prompt: "Suggest 3 great shows or movies to watch tonight".to_string(),
            gemini_reply: String::new(),
            gemini_last_id: None,
            gemini_busy: false,
            gemini_text_mode: false,
            gemini_listening: false,
            app_version: bridge::get_app_version(),
            display_hz: bridge::get_refresh_rate(),
            fps_frame_count: 0,
            fps_window_start: now,
            fps_current: 60.0,
            fps_overlay_active: false,
            updater_state: UpdaterState::Idle,
            focus: FocusTarget::App(0),
            last_app_idx: 0,
            focus_changed: false,
            focus_changed_at: now - Duration::from_secs(2),
            modal: if show_welcome {
                ActiveModal::Welcome
            } else {
                ActiveModal::None
            },
            modal_stack: Vec::new(),
            modal_focus: if show_welcome { 4 } else { 0 },
            modal_close_focused: false,
            modal_just_opened: false,
            modal_wait_key_release: false,
            modal_focus_changed: false,
            modal_opened_at: now,
            last_back_handled: now - Duration::from_secs(2),
            last_sat_refresh: now,
            settings_tab: 0,
            settings_zone: SettingsZone::List,
            settings_ribbon_focus: 0,
            settings_sys_col: 0,
            select_down_since: None,
            select_long_triggered: false,
            search_query: String::new(),
            rename_input: String::new(),
            toast: None,
            tx,
            rx,
            web_rx,
            config_snapshot: config_snapshot.clone(),
            last_input_at: now,
            ambient_active: false,
            active_folder_idx: None,
            startup_pending,
            startup_started_at: now,
            start_time: now,
            last_status_poll: now - Duration::from_secs(60),
            last_weather_poll: now - Duration::from_secs(900),
            last_grid_cols: 6,
            pending_apps_refresh: false,
        };

        if web_enabled {
            web_server::spawn(
                web_tx,
                cc.egui_ctx.clone(),
                bridge::get_cache_dir(),
                config_snapshot,
            );
        }
        if app.config.auto_clear_ram_on_home {
            app.spawn_clear_ram(cc.egui_ctx.clone());
        }

        app.spawn_load_apps(cc.egui_ctx.clone());
        app.spawn_load_status(cc.egui_ctx.clone());
        app.spawn_load_weather(cc.egui_ctx.clone());
        app.ensure_wallpaper(cc.egui_ctx.clone());
        app.spawn_check_update(cc.egui_ctx.clone(), true);
        if app.config.show_fps_meter {
            app.fps_overlay_active =
                bridge::set_fps_overlay(true, app.config.overlay_flags()) == "ON";
        }
        app
    }

    fn save_config(&mut self) {
        self.rebuild_app_lists();
        if let Ok(json) = serde_json::to_string(&self.config) {
            bridge::set_pref(PREF_CONFIG_KEY, &json);
        }
        if let Ok(pretty) = serde_json::to_string_pretty(&self.config) {
            if let Ok(mut g) = self.config_snapshot.lock() {
                *g = pretty;
            }
        }
    }

    fn update_clock_cache(&mut self) {
        let now = Local::now();
        let sec = now.second();
        if sec != self.cached_sec || self.cached_time_str.is_empty() {
            self.cached_sec = sec;
            self.cached_time_str = if self.config.clock_24h {
                now.format("%H:%M:%S").to_string()
            } else {
                now.format("%I:%M:%S %p").to_string()
            };
            self.cached_date_str = format!("{}  •  {}", now.format("%A, %d %B %Y"), self.config.weather_city);
            self.cached_date_short = now.format("%a %d %b").to_string();
        }
    }

    fn rebuild_app_lists(&mut self) {
        let vis = self.visible_apps();
        let favs = self.favorite_apps(&vis);
        self.cached_visible = Arc::new(vis);
        self.cached_favs = Arc::new(favs);
        // Reset clock cache in case 24h toggle changed
        self.cached_sec = u32::MAX;
        // Clamp focus safely if apps/favorites shrank
        match self.focus {
            FocusTarget::Fav(i) => {
                if self.cached_favs.is_empty() {
                    self.focus = if !self.cached_visible.is_empty() {
                        FocusTarget::App(0)
                    } else {
                        FocusTarget::Dock(0)
                    };
                } else if i >= self.cached_favs.len() {
                    self.focus = FocusTarget::Fav(self.cached_favs.len() - 1);
                }
            }
            FocusTarget::App(i) => {
                if self.cached_visible.is_empty() {
                    self.focus = FocusTarget::Dock(0);
                } else if i >= self.cached_visible.len() {
                    self.focus = FocusTarget::App(self.cached_visible.len() - 1);
                }
            }
            _ => {}
        }
    }

    fn show_toast(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), Instant::now()));
    }

    fn open_modal(&mut self, modal: ActiveModal) {
        if modal == ActiveModal::Settings && self.modal == ActiveModal::None {
            self.settings_tab = 0;
            self.settings_zone = SettingsZone::List;
            self.settings_ribbon_focus = 0;
        }
        if modal == ActiveModal::None {
            self.modal_stack.clear();
        } else if self.modal != ActiveModal::None && self.modal != modal {
            self.modal_stack.push((self.modal.clone(), self.modal_focus));
        }
        self.modal = modal;
        self.modal_focus = 0;
        self.modal_close_focused = false;
        self.modal_just_opened = true;
        self.modal_wait_key_release = true;
        self.modal_focus_changed = true;
        self.modal_opened_at = Instant::now();
    }

    fn close_modal_step(&mut self) {
        if self.modal == ActiveModal::Welcome {
            self.config.onboarding_done = true;
            self.save_config();
        }
        if self.modal == ActiveModal::Gemini {
            if self.gemini_listening {
                let _ = bridge::stop_voice_recording();
                self.gemini_listening = false;
            }
            bridge::stop_speaking();
        }
        self.modal_close_focused = false;
        if let Some((prev_modal, prev_focus)) = self.modal_stack.pop() {
            if prev_modal == ActiveModal::Settings {
                self.settings_zone = SettingsZone::List;
            }
            self.modal = prev_modal;
            self.modal_focus = prev_focus;
            self.modal_just_opened = true;
            self.modal_wait_key_release = true;
            self.modal_focus_changed = true;
            self.modal_opened_at = Instant::now();
        } else {
            self.modal = ActiveModal::None;
        }
    }

    fn display_label(&self, app: &TvAppInfo) -> String {
        self.config
            .custom_labels
            .get(&app.package_name)
            .cloned()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| app.label.clone())
    }

    fn active_scene(&self) -> WeatherScene {
        if self.config.weather_scene != WeatherScene::Auto {
            return self.config.weather_scene;
        }
        if let Some(w) = &self.weather {
            w.scene
        } else {
            let h = Local::now().format("%H").to_string().parse::<u32>().unwrap_or(12);
            if (6..=7).contains(&h) || (16..=18).contains(&h) {
                WeatherScene::Golden
            } else if (8..=15).contains(&h) {
                WeatherScene::ClearDay
            } else {
                WeatherScene::ClearNight
            }
        }
    }

    fn force_refresh_all(&mut self, ctx: &egui::Context) {
        self.spawn_load_apps(ctx.clone());
        self.spawn_load_status(ctx.clone());
        self.spawn_load_weather(ctx.clone());
        let sc = self.active_scene();
        if matches!(sc, WeatherScene::LiveUkSat | WeatherScene::LiveEarth) {
            self.last_sat_refresh = Instant::now();
            self.wallpaper_loading = None;
            self.wallpaper_tex = None;
            self.ensure_wallpaper(ctx.clone());
        }
        self.show_toast("Refreshed apps, weather & live wallpaper");
    }

    fn spawn_load_apps(&self, ctx: egui::Context) {
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let apps = bridge::get_installed_apps();
            let _ = tx.send(BgMsg::AppsLoaded(apps));
            ctx.request_repaint();
        });
    }

    fn spawn_load_status(&mut self, ctx: egui::Context) {
        self.last_status_poll = Instant::now();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let status = bridge::get_system_status();
            let is_home = bridge::is_default_home();
            let music = bridge::is_music_active();
            let _ = tx.send(BgMsg::StatusLoaded(status, is_home, music));
            ctx.request_repaint();
        });
    }

    fn spawn_clear_ram(&mut self, ctx: egui::Context) {
        let before = self.status.ram_free_mb;
        let tx = self.tx.clone();
        self.show_toast("Clearing background RAM...");
        std::thread::spawn(move || {
            let after = bridge::clear_ram();
            let _ = tx.send(BgMsg::RamCleared(before, after));
            ctx.request_repaint();
        });
    }

    fn has_update_available(&self) -> bool {
        matches!(
            self.updater_state,
            UpdaterState::Available(_)
                | UpdaterState::Downloading { .. }
                | UpdaterState::ReadyToInstall { .. }
                | UpdaterState::NeedPermission { .. }
        )
    }

    fn spawn_check_update(&mut self, ctx: egui::Context, silent: bool) {
        if matches!(
            self.updater_state,
            UpdaterState::Checking | UpdaterState::Downloading { .. }
        ) {
            return;
        }
        if !silent {
            self.updater_state = UpdaterState::Checking;
        }
        let current = self.app_version.clone();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let outcome = updater::check_for_update(&current);
            let _ = tx.send(BgMsg::UpdateChecked(outcome, silent));
            ctx.request_repaint();
        });
    }

    fn spawn_download_update(&mut self, ctx: egui::Context, release: ReleaseInfo) {
        if matches!(self.updater_state, UpdaterState::Downloading { .. }) {
            return;
        }
        let url = release.apk_url.clone();
        let expected_size = release.apk_size;
        self.updater_state = UpdaterState::Downloading {
            release,
            downloaded: 0,
            total: expected_size,
        };
        let cache_dir = bridge::get_cache_dir();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let tx_prog = tx.clone();
            let ctx_prog = ctx.clone();
            let res = updater::download_apk(&url, &cache_dir, expected_size, move |dl, tot| {
                let _ = tx_prog.send(BgMsg::UpdateDownloadProgress(dl, tot));
                ctx_prog.request_repaint();
            });
            let _ = tx.send(BgMsg::UpdateDownloadDone(res));
            ctx.request_repaint();
        });
    }

    fn trigger_install_apk(&mut self, release: ReleaseInfo, apk_path: String) {
        let res = bridge::install_apk(&apk_path);
        if res == "OK" {
            self.updater_state = UpdaterState::ReadyToInstall { release, apk_path };
            self.show_toast("Launching Android TV installer...");
        } else if res == "NEED_PERMISSION" {
            self.updater_state = UpdaterState::NeedPermission { release, apk_path };
            self.show_toast("Enable 'Install unknown apps' for Astryx, then press Install");
        } else {
            let msg = res.strip_prefix("ERR:").unwrap_or(&res).to_string();
            self.updater_state = UpdaterState::Error(msg.clone());
            self.show_toast(format!("Install error: {msg}"));
        }
    }

    fn spawn_load_weather(&mut self, ctx: egui::Context) {
        self.last_weather_poll = Instant::now();
        let lat = self.config.weather_lat;
        let lon = self.config.weather_lon;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            if let Some(w) = weather::fetch_weather_for(lat, lon) {
                let _ = tx.send(BgMsg::WeatherLoaded(w));
                ctx.request_repaint();
            }
        });
    }

    fn ensure_wallpaper(&mut self, ctx: egui::Context) {
        let scene = self.active_scene();
        if scene.is_procedural_shader() {
            self.wallpaper_tex = None;
            self.wallpaper_loading = None;
            return;
        }
        let blur = self.config.bg_blur;
        if let Some((loaded_scene, loaded_blur, _)) = &self.wallpaper_tex {
            if *loaded_scene == scene && *loaded_blur == blur {
                return;
            }
        }
        if self.wallpaper_loading == Some((scene, blur)) {
            return;
        }
        self.wallpaper_loading = Some((scene, blur));
        let custom_path = self.config.custom_wallpaper_path.clone();
        let lightweight = self.config.lightweight_mode;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            if let Some(img) = weather::fetch_wallpaper_image(scene, blur, &custom_path, lightweight) {
                let _ = tx.send(BgMsg::WallpaperLoaded(scene, blur, img));
                ctx.request_repaint();
            }
        });
    }

    fn request_icon(&mut self, pkg: &str, ctx: &egui::Context) {
        if self.icon_textures.contains_key(pkg) || self.icon_loading.contains(pkg) {
            return;
        }
        self.icon_loading.insert(pkg.to_string());
        let tx = self.tx.clone();
        let pkg_owned = pkg.to_string();
        let ctx_clone = ctx.clone();
        std::thread::spawn(move || {
            if let Some(img) = bridge::get_app_icon_circular(&pkg_owned) {
                let _ = tx.send(BgMsg::IconLoaded(pkg_owned, img));
                ctx_clone.request_repaint();
            }
        });
    }

    fn drain_messages(&mut self, ctx: &egui::Context) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                BgMsg::AppsLoaded(apps) => {
                    self.apps = apps;
                    self.rebuild_app_lists();
                    if matches!(self.focus, FocusTarget::App(0)) && self.cached_visible.is_empty() {
                        self.focus = FocusTarget::Dock(0);
                    }
                    // Batch-load all visible app icons on 1 clean worker thread (no 11-thread spawn storm!)
                    let pkgs: Vec<String> = self
                        .cached_visible
                        .iter()
                        .filter(|a| !self.icon_textures.contains_key(&a.package_name) && !self.icon_loading.contains(&a.package_name))
                        .map(|a| a.package_name.clone())
                        .collect();
                    for p in &pkgs {
                        self.icon_loading.insert(p.clone());
                    }
                    if !pkgs.is_empty() {
                        let tx = self.tx.clone();
                        let ctx_c = ctx.clone();
                        std::thread::spawn(move || {
                            for pkg in pkgs {
                                if let Some(img) = bridge::get_app_icon_circular(&pkg) {
                                    let _ = tx.send(BgMsg::IconLoaded(pkg, img));
                                    ctx_c.request_repaint();
                                }
                            }
                        });
                    }
                }
                BgMsg::IconLoaded(pkg, img) => {
                    self.icon_loading.remove(&pkg);
                    let tex = ctx.load_texture(
                        format!("icon_{pkg}"),
                        img,
                        TextureOptions::LINEAR,
                    );
                    self.icon_textures.insert(pkg, tex);
                }
                BgMsg::StatusLoaded(st, is_home, music) => {
                    self.status = st;
                    self.is_default_home = is_home;
                    self.is_music_active = music;
                }
                BgMsg::WeatherLoaded(w) => {
                    self.weather = Some(w);
                    self.ensure_wallpaper(ctx.clone());
                }
                BgMsg::CityGeocoded(label, lat, lon) => {
                    self.config.weather_city = label.clone();
                    self.config.weather_lat = lat;
                    self.config.weather_lon = lon;
                    self.save_config();
                    self.spawn_load_weather(ctx.clone());
                    self.show_toast(format!("Weather set to {label}"));
                }
                BgMsg::WallpaperLoaded(scene, blur, img) => {
                    if self.wallpaper_loading == Some((scene, blur)) {
                        self.wallpaper_loading = None;
                    }
                    let tex = ctx.load_texture(
                        format!("wp_{scene:?}_{blur}"),
                        img,
                        TextureOptions::LINEAR,
                    );
                    self.wallpaper_tex = Some((scene, blur, tex));
                }
                BgMsg::RamCleared(before, after) => {
                    if after > 0 {
                        self.status.ram_free_mb = after;
                        let freed = (after - before).max(0);
                        if freed > 0 {
                            self.show_toast(format!("✓ RAM Cleared — +{freed} MB freed ({after} MB free)"));
                        } else {
                            self.show_toast(format!("✓ RAM Optimized — {after} MB free"));
                        }
                    } else {
                        self.show_toast("✓ RAM Optimized");
                    }
                }
                BgMsg::UpdateChecked(outcome, silent) => match outcome {
                    UpdateCheckOutcome::Available(rel) => {
                        let tag = rel.tag.clone();
                        self.updater_state = UpdaterState::Available(rel);
                        if silent {
                            self.show_toast(format!("Update {tag} available — open Settings → App Updates"));
                        }
                    }
                    UpdateCheckOutcome::UpToDate { latest, .. } => {
                        if !silent || !self.has_update_available() {
                            self.updater_state = UpdaterState::UpToDate { latest };
                        }
                    }
                    UpdateCheckOutcome::Error(err) => {
                        if !silent {
                            self.updater_state = UpdaterState::Error(err);
                        }
                    }
                },
                BgMsg::UpdateDownloadProgress(dl, tot) => {
                    if let UpdaterState::Downloading {
                        downloaded, total, ..
                    } = &mut self.updater_state
                    {
                        *downloaded = dl;
                        *total = tot;
                    }
                }
                BgMsg::UpdateDownloadDone(res) => match res {
                    Ok(path) => {
                        if let UpdaterState::Downloading { release, .. } = self.updater_state.clone() {
                            self.trigger_install_apk(release, path);
                        }
                    }
                    Err(e) => {
                        self.updater_state = UpdaterState::Error(e.clone());
                        self.show_toast(format!("Update failed: {e}"));
                    }
                },
                BgMsg::GeminiReplied(res) => {
                    self.gemini_busy = false;
                    match res {
                        Ok(reply) => {
                            if !reply.interaction_id.is_empty() {
                                self.gemini_last_id = Some(reply.interaction_id);
                            }
                            let text_copy = reply.text.clone();
                            self.gemini_reply = reply.text;
                            if !self.gemini_text_mode {
                                let played = reply
                                    .tts_wav
                                    .as_deref()
                                    .map(bridge::play_wav_bytes)
                                    .unwrap_or(false);
                                if !played {
                                    bridge::speak_fallback_tts(&text_copy);
                                }
                                self.a2ui_feedback =
                                    format!("🔊 Speaking ({})", self.config.gemini_model);
                            } else {
                                self.a2ui_feedback =
                                    format!("✓ Gemini ({}) replied", self.config.gemini_model);
                            }
                        }
                        Err(e) => {
                            self.a2ui_feedback = format!("Gemini error: {e}");
                            self.show_toast(format!("Gemini error: {e}"));
                        }
                    }
                }
                BgMsg::GeminiA2uiReady(res) => {
                    self.gemini_busy = false;
                    match res {
                        Ok(json_payload) => {
                            self.a2ui_input = json_payload.clone();
                            match self.a2ui.process_json(&json_payload) {
                                Ok(n) => {
                                    self.a2ui_feedback = format!(
                                        "✓ Gemini generated & applied {n} live A2UI surface message(s)"
                                    );
                                    self.show_toast("✓ Gemini A2UI shelf added to Home!");
                                }
                                Err(e) => {
                                    self.a2ui_feedback =
                                        format!("Gemini JSON parse error: {e}");
                                }
                            }
                        }
                        Err(e) => {
                            self.a2ui_feedback = format!("Gemini error: {e}");
                            self.show_toast(format!("Gemini error: {e}"));
                        }
                    }
                }
            }
        }

        // Drain Phone Wi-Fi Web Companion commands
        let cmds: Vec<WebCmd> = self.web_rx.try_iter().collect();
        for cmd in cmds {
            self.last_input_at = Instant::now();
            self.ambient_active = false;
            match cmd {
                WebCmd::ClearRam => self.spawn_clear_ram(ctx.clone()),
                WebCmd::ToggleOverlay => {
                    self.config.show_fps_meter = !self.config.show_fps_meter;
                    self.save_config();
                    let res = bridge::set_fps_overlay(
                        self.config.show_fps_meter,
                        self.config.overlay_flags(),
                    );
                    self.fps_overlay_active = res == "ON";
                    self.show_toast(if self.config.show_fps_meter {
                        "HUD Overlay ON"
                    } else {
                        "HUD Overlay OFF"
                    });
                }
                WebCmd::TriggerSleep => bridge::trigger_sleep(),
                WebCmd::SetCity(q) => {
                    let tx = self.tx.clone();
                    let ctx_c = ctx.clone();
                    std::thread::spawn(move || {
                        if let Some((lbl, lat, lon)) = weather::geocode_city(&q) {
                            let _ = tx.send(BgMsg::CityGeocoded(lbl, lat, lon));
                            ctx_c.request_repaint();
                        }
                    });
                }
                WebCmd::SetTheme(name) => {
                    self.config.astryx_theme = match name.as_str() {
                        "Y2k" => AstryxThemeId::Y2k,
                        "Matcha" => AstryxThemeId::Matcha,
                        "Neutral" => AstryxThemeId::Neutral,
                        _ => AstryxThemeId::Gothic,
                    };
                    self.save_config();
                    self.show_toast(format!("Theme: {}", self.config.astryx_theme.label()));
                }
                WebCmd::SetScene(name) => {
                    self.config.weather_scene = match name.as_str() {
                        "AuroraBorealis" => WeatherScene::AuroraBorealis,
                        "LiquidMesh" => WeatherScene::LiquidMesh,
                        "DeepSpace" => WeatherScene::DeepSpace,
                        "LiveEarth" => WeatherScene::LiveEarth,
                        "LiveUkSat" => WeatherScene::LiveUkSat,
                        _ => WeatherScene::Auto,
                    };
                    self.save_config();
                    self.ensure_wallpaper(ctx.clone());
                    self.show_toast(format!("Wallpaper: {}", self.config.weather_scene.label()));
                }
                WebCmd::CustomWallpaperUploaded(path) => {
                    self.config.custom_wallpaper_path = path;
                    self.config.weather_scene = WeatherScene::CustomLocal;
                    self.wallpaper_tex = None;
                    self.wallpaper_loading = None;
                    self.save_config();
                    self.ensure_wallpaper(ctx.clone());
                    self.show_toast("Applied custom 4K wallpaper!");
                }
                WebCmd::PushA2ui(payload) => {
                    if let Ok(n) = self.a2ui.process_json(&payload) {
                        self.show_toast(format!("A2UI: Applied {n} surface message(s)"));
                    }
                }
                WebCmd::SetGeminiKey(key) => {
                    self.config.gemini_api_key = key;
                    self.gemini_last_id = None;
                    self.save_config();
                    self.show_toast(if self.config.gemini_api_key.is_empty() {
                        "Cleared Gemini API key"
                    } else {
                        "✓ Saved Gemini API key"
                    });
                }
                WebCmd::SetGeminiModel(model) => {
                    if !model.is_empty() {
                        self.config.gemini_model = model;
                        self.gemini_last_id = None;
                        self.save_config();
                        self.show_toast(format!("Gemini model: {}", self.config.gemini_model));
                    }
                }
                WebCmd::AskGemini(prompt) => {
                    if !self.config.gemini_enabled {
                        self.show_toast("Gemini AI is disabled in Settings");
                    } else {
                        self.gemini_prompt = prompt.clone();
                        self.open_modal(ActiveModal::Gemini);
                        self.spawn_ask_gemini(ctx.clone(), prompt);
                    }
                }
                WebCmd::AskGeminiVoice(audio_bytes, mime) => {
                    if !self.config.gemini_enabled {
                        self.show_toast("Gemini AI is disabled in Settings");
                    } else {
                        self.gemini_text_mode = false;
                        self.open_modal(ActiveModal::Gemini);
                        self.spawn_ask_gemini_voice(ctx.clone(), audio_bytes, mime);
                    }
                }
                WebCmd::GeminiA2ui(prompt) => {
                    if !self.config.gemini_enabled {
                        self.show_toast("Gemini AI is disabled in Settings");
                    } else {
                        self.gemini_prompt = prompt.clone();
                        self.spawn_gemini_a2ui(ctx.clone(), prompt);
                    }
                }
                WebCmd::SetScreensaverStyle(name) => {
                    self.config.screensaver_style = match name.as_str() {
                        "GlassCard" => ScreensaverStyle::GlassCard,
                        "MinimalClock" => ScreensaverStyle::MinimalClock,
                        "EditorialSplit" => ScreensaverStyle::EditorialSplit,
                        "NeonRing" => ScreensaverStyle::NeonRing,
                        _ => ScreensaverStyle::Classic,
                    };
                    self.save_config();
                    self.show_toast(format!(
                        "Screensaver Style: {}",
                        self.config.screensaver_style.label()
                    ));
                }
                WebCmd::ImportConfigJson(json) => {
                    if let Ok(cfg) = serde_json::from_str::<LauncherConfig>(&json) {
                        self.config = cfg;
                        self.save_config();
                        self.spawn_load_weather(ctx.clone());
                        self.ensure_wallpaper(ctx.clone());
                        self.show_toast("Restored backup configuration!");
                    }
                }
            }
        }
    }

    fn tv_context_summary(&self) -> String {
        let w = self
            .weather
            .as_ref()
            .map(|w| format!("{}, {} ({}°C)", self.config.weather_city, w.condition, w.temp_c))
            .unwrap_or_else(|| self.config.weather_city.clone());
        let fav_names: Vec<String> = self
            .cached_favs
            .iter()
            .take(6)
            .map(|a| self.display_label(a))
            .collect();
        format!(
            "Time: {} {}; Weather: {}; Installed apps: {}; Pinned apps: [{}]",
            self.cached_time_str,
            self.cached_date_str,
            w,
            self.apps.len(),
            fav_names.join(", ")
        )
    }

    fn start_gemini_voice_listen(&mut self) {
        if !self.config.gemini_enabled {
            self.a2ui_feedback = "Gemini AI is disabled in Settings".to_string();
            return;
        }
        if self.config.gemini_api_key.trim().is_empty() {
            self.a2ui_feedback =
                "Add your Gemini API key via Phone Companion first".to_string();
            return;
        }
        bridge::stop_speaking();
        let status = bridge::start_voice_recording();
        if status == "OK" {
            self.gemini_listening = true;
            self.a2ui_feedback = "🎤 Listening — speak now (auto-sends on pause, or press OK)".to_string();
        } else if status == "NEED_MIC_PERMISSION" {
            self.gemini_listening = false;
            self.a2ui_feedback = "Allow microphone access on TV, then press Speak again".to_string();
        } else {
            self.gemini_listening = false;
            self.a2ui_feedback = status.trim_start_matches("ERR:").to_string();
        }
    }

    fn finish_gemini_voice_listen(&mut self, ctx: egui::Context) {
        self.gemini_listening = false;
        let wav = bridge::stop_voice_recording();
        if wav.len() < 800 {
            self.a2ui_feedback = "Didn't catch that — press Speak and talk closer to the mic".to_string();
            return;
        }
        self.spawn_ask_gemini_voice(ctx, wav, "audio/wav".to_string());
    }

    fn spawn_ask_gemini_voice(&mut self, ctx: egui::Context, audio: Vec<u8>, mime: String) {
        if !self.config.gemini_enabled {
            self.a2ui_feedback = "Gemini AI is disabled in Settings".to_string();
            self.show_toast("Gemini AI is disabled in Settings");
            return;
        }
        if self.gemini_busy {
            return;
        }
        let api_key = self.config.gemini_api_key.clone();
        let model = self.config.gemini_model.clone();
        let prev_id = self.gemini_last_id.clone();
        let tv_ctx = self.tv_context_summary();
        let tx = self.tx.clone();
        self.gemini_busy = true;
        self.a2ui_feedback = format!("✦ Gemini ({model}) processing voice...");
        std::thread::spawn(move || {
            let res = gemini::ask_tv_voice_assistant(
                &api_key,
                &model,
                &audio,
                &mime,
                prev_id.as_deref(),
                &tv_ctx,
            );
            let _ = tx.send(BgMsg::GeminiReplied(res));
            ctx.request_repaint();
        });
    }

    fn spawn_ask_gemini(&mut self, ctx: egui::Context, prompt: String) {
        if !self.config.gemini_enabled {
            self.a2ui_feedback = "Gemini AI is disabled in Settings".to_string();
            self.show_toast("Gemini AI is disabled in Settings");
            return;
        }
        if self.gemini_busy {
            return;
        }
        bridge::stop_speaking();
        let api_key = self.config.gemini_api_key.clone();
        let model = self.config.gemini_model.clone();
        let prev_id = self.gemini_last_id.clone();
        let tv_ctx = self.tv_context_summary();
        let with_tts = !self.gemini_text_mode;
        let tx = self.tx.clone();
        self.gemini_busy = true;
        self.a2ui_feedback = format!("✦ Gemini ({model}) thinking...");
        std::thread::spawn(move || {
            let res = gemini::ask_tv_assistant(
                &api_key,
                &model,
                &prompt,
                prev_id.as_deref(),
                &tv_ctx,
                with_tts,
            );
            let _ = tx.send(BgMsg::GeminiReplied(res));
            ctx.request_repaint();
        });
    }

    fn spawn_gemini_a2ui(&mut self, ctx: egui::Context, prompt: String) {
        if !self.config.gemini_enabled {
            self.a2ui_feedback = "Gemini AI is disabled in Settings".to_string();
            self.show_toast("Gemini AI is disabled in Settings");
            return;
        }
        if self.gemini_busy {
            return;
        }
        let api_key = self.config.gemini_api_key.clone();
        let model = self.config.gemini_model.clone();
        let tv_ctx = self.tv_context_summary();
        let tx = self.tx.clone();
        self.gemini_busy = true;
        self.a2ui_feedback = format!("🧩 Gemini ({model}) building A2UI surface...");
        std::thread::spawn(move || {
            let res = gemini::generate_a2ui_surface(&api_key, &model, &prompt, &tv_ctx);
            let _ = tx.send(BgMsg::GeminiA2uiReady(res));
            ctx.request_repaint();
        });
    }

    fn visible_apps(&self) -> Vec<TvAppInfo> {
        let mut list: Vec<TvAppInfo> = self
            .apps
            .iter()
            .filter(|a| {
                if self.config.hidden_apps.contains(&a.package_name) {
                    return false;
                }
                if !self.config.show_system_apps && a.is_system_app && !a.is_leanback {
                    return false;
                }
                if !self.config.show_mobile_apps && !a.is_leanback {
                    return false;
                }
                true
            })
            .cloned()
            .collect();

        match self.config.sort_order {
            SortOrder::Alpha => {
                list.sort_by(|a, b| {
                    self.display_label(a)
                        .to_lowercase()
                        .cmp(&self.display_label(b).to_lowercase())
                });
            }
            SortOrder::Recent => {
                list.sort_by(|a, b| {
                    let ra = self
                        .config
                        .recents
                        .iter()
                        .position(|p| p == &a.package_name)
                        .unwrap_or(usize::MAX);
                    let rb = self
                        .config
                        .recents
                        .iter()
                        .position(|p| p == &b.package_name)
                        .unwrap_or(usize::MAX);
                    ra.cmp(&rb).then_with(|| {
                        self.display_label(a)
                            .to_lowercase()
                            .cmp(&self.display_label(b).to_lowercase())
                    })
                });
            }
        }

        // Apply custom app order if user has reordered apps
        if !self.config.custom_order.is_empty() {
            list.sort_by_key(|a| {
                self.config
                    .custom_order
                    .iter()
                    .position(|p| p == &a.package_name)
                    .unwrap_or(usize::MAX)
            });
        }

        // In Zenith Horizon, keep Pinned Favorites at the front in their exact pinned order
        if self.config.layout_style == LayoutStyle::Zenith && !self.config.favorites.is_empty() {
            list.sort_by_key(|a| {
                self.config
                    .favorites
                    .iter()
                    .position(|p| p == &a.package_name)
                    .unwrap_or(usize::MAX)
            });
        }

        list
    }

    fn favorite_apps(&self, visible: &[TvAppInfo]) -> Vec<TvAppInfo> {
        let mut favs = Vec::new();
        for pkg in &self.config.favorites {
            if let Some(app) = visible.iter().find(|a| &a.package_name == pkg) {
                favs.push(app.clone());
            }
        }
        favs
    }

    fn launch_app(&mut self, pkg: &str) {
        self.config.recents.retain(|p| p != pkg);
        self.config.recents.insert(0, pkg.to_string());
        self.config.recents.truncate(24);
        self.save_config();
        if !bridge::launch_app(pkg) {
            self.show_toast(format!("Could not launch {pkg}"));
        }
    }

    fn open_vpn_app(&mut self) {
        let vpn_pkg = self
            .apps
            .iter()
            .find(|a| {
                let p = a.package_name.to_lowercase();
                let l = a.label.to_lowercase();
                p.contains("nordvpn") || l.contains("nordvpn") || p.contains("vpn") || l.contains("vpn")
            })
            .map(|a| a.package_name.clone());

        if let Some(pkg) = vpn_pkg {
            self.launch_app(&pkg);
        } else {
            bridge::open_system_settings("network");
        }
    }

    fn toggle_favorite(&mut self, pkg: &str) {
        if let Some(pos) = self.config.favorites.iter().position(|p| p == pkg) {
            self.config.favorites.remove(pos);
            self.show_toast("Removed from Pinned Favorites");
        } else {
            self.config.favorites.push(pkg.to_string());
            self.show_toast("Pinned to Favorites");
        }
        self.save_config();
    }

    fn move_app_order(&mut self, pkg: &str, delta: isize) {
        if let Some(pos) = self.config.favorites.iter().position(|p| p == pkg) {
            let new_pos = (pos as isize + delta)
                .clamp(0, self.config.favorites.len().saturating_sub(1) as isize)
                as usize;
            if new_pos != pos {
                let item = self.config.favorites.remove(pos);
                self.config.favorites.insert(new_pos, item);
                self.save_config();
            }
            return;
        }
        // Reorder in custom_order (seeded from current visible apps if empty)
        if self.config.custom_order.is_empty() {
            self.config.custom_order = self
                .cached_visible
                .iter()
                .map(|a| a.package_name.clone())
                .collect();
        }
        if !self.config.custom_order.iter().any(|p| p == pkg) {
            self.config.custom_order.push(pkg.to_string());
        }
        if let Some(pos) = self.config.custom_order.iter().position(|p| p == pkg) {
            let new_pos = (pos as isize + delta)
                .clamp(0, self.config.custom_order.len().saturating_sub(1) as isize)
                as usize;
            if new_pos != pos {
                let item = self.config.custom_order.remove(pos);
                self.config.custom_order.insert(new_pos, item);
                self.save_config();
            }
        }
    }

    fn hide_app(&mut self, pkg: &str) {
        if !self.config.hidden_apps.contains(&pkg.to_string()) {
            self.config.hidden_apps.push(pkg.to_string());
            self.config.favorites.retain(|p| p != pkg);
            self.save_config();
            self.show_toast("App hidden (restore in Settings)");
        }
    }

    fn fmt_temp(&self, temp_c: i32) -> String {
        if self.use_fahrenheit {
            format!("{}°F", ((temp_c as f32 * 9.0 / 5.0) + 32.0).round() as i32)
        } else {
            format!("{temp_c}°C")
        }
    }

    fn handle_home_keys(
        &mut self,
        ctx: &egui::Context,
        favs: &[TvAppInfo],
        visible: &[TvAppInfo],
    ) {
        let mut left = false;
        let mut right = false;
        let mut up = false;
        let mut down = false;
        let mut select_pressed = false;
        let mut select_held = false;
        let mut select_released = false;
        let mut menu = false;

        ctx.input(|i| {
            left = i.key_pressed(Key::ArrowLeft);
            right = i.key_pressed(Key::ArrowRight);
            up = i.key_pressed(Key::ArrowUp);
            down = i.key_pressed(Key::ArrowDown);
            select_pressed = i.key_pressed(Key::Enter) || i.key_pressed(Key::Space);
            select_held = i.key_down(Key::Enter) || i.key_down(Key::Space);
            select_released = i.key_released(Key::Enter) || i.key_released(Key::Space);
            menu = i.key_pressed(Key::F1) || i.key_pressed(Key::M) || i.key_pressed(Key::Tab);
        });
        if bridge::take_menu_pressed() {
            menu = true;
        }
        // Clear any stray back press on home screen (keeps launcher open; or returns focus to apps if in top bar)
        if bridge::take_back_pressed() {
            if matches!(self.focus, FocusTarget::Dock(_) | FocusTarget::Weather) && !visible.is_empty() {
                self.focus = FocusTarget::App(0);
                self.focus_changed = true;
            }
        }

        let on_app_tile = matches!(self.focus, FocusTarget::App(_) | FocusTarget::Fav(_));
        let mut select = false;

        if on_app_tile {
            if select_pressed {
                if self.select_down_since.is_none() {
                    self.select_down_since = Some(Instant::now());
                    self.select_long_triggered = false;
                } else if !self.select_long_triggered {
                    // Android TV remote sends auto-repeat Enter while OK is held -> open App Options immediately!
                    self.select_long_triggered = true;
                    menu = true;
                }
            }
            if select_held {
                if let Some(since) = self.select_down_since {
                    if !self.select_long_triggered && since.elapsed() >= Duration::from_millis(380) {
                        self.select_long_triggered = true;
                        menu = true;
                    } else if !self.select_long_triggered {
                        ctx.request_repaint_after(Duration::from_millis(50));
                    }
                }
            }
            if select_released {
                if !self.select_long_triggered && self.select_down_since.is_some() {
                    select = true;
                }
                self.select_down_since = None;
                self.select_long_triggered = false;
            }
        } else {
            self.select_down_since = None;
            self.select_long_triggered = false;
            select = select_pressed;
        }

        if !(left || right || up || down || select || menu) {
            return;
        }

        let dock = self.dock_items();
        let dock_last = dock.len().saturating_sub(1);
        let has_weather = self.config.show_header && self.config.show_weather;
        let has_favs = !favs.is_empty();
        let app_len = visible.len();
        let cols = self.last_grid_cols.max(1);
        let prev = self.focus.clone();

        if select {
            match &self.focus {
                FocusTarget::Dock(pos) => {
                    if let Some(&(action_id, _)) = dock.get(*pos) {
                        self.trigger_dock_action(action_id, ctx);
                    }
                }
                FocusTarget::Weather => self.open_modal(ActiveModal::Weather),
                FocusTarget::Fav(i) => {
                    if let Some(a) = favs.get(*i) {
                        let pkg = a.package_name.clone();
                        self.launch_app(&pkg);
                    }
                }
                FocusTarget::App(i) => {
                    if let Some(a) = visible.get(*i) {
                        let pkg = a.package_name.clone();
                        self.launch_app(&pkg);
                    }
                }
                FocusTarget::RamPill => {
                    self.spawn_clear_ram(ctx.clone());
                }
            }
            return;
        }

        if menu {
            match &self.focus {
                FocusTarget::Fav(i) => {
                    if let Some(a) = favs.get(*i) {
                        self.open_modal(ActiveModal::AppContext(a.package_name.clone()));
                    }
                }
                FocusTarget::App(i) => {
                    if let Some(a) = visible.get(*i) {
                        self.open_modal(ActiveModal::AppContext(a.package_name.clone()));
                    }
                }
                _ => {}
            }
            return;
        }

        let is_zenith = self.config.layout_style == LayoutStyle::Zenith;

        match self.focus.clone() {
            FocusTarget::Dock(i) => {
                let cur = i.min(dock_last);
                if left {
                    if cur > 0 {
                        self.focus = FocusTarget::Dock(cur - 1);
                    } else if is_zenith && has_weather {
                        // In Zenith unified top bar, Weather sits directly left of Wi-Fi
                        self.focus = FocusTarget::Weather;
                    }
                    // In Classic mode, Left on Wi-Fi (0) stops strictly at edge!
                } else if right {
                    self.focus = FocusTarget::Dock((cur + 1).min(dock_last));
                } else if down {
                    if is_zenith {
                        if app_len > 0 {
                            self.focus = FocusTarget::App(0);
                        }
                    } else if has_weather {
                        self.focus = FocusTarget::Weather;
                    } else if has_favs {
                        self.focus = FocusTarget::Fav(0);
                    } else if app_len > 0 {
                        self.focus = FocusTarget::App(0);
                    }
                }
            }
            FocusTarget::Weather => {
                if is_zenith {
                    if right {
                        self.focus = FocusTarget::Dock(0);
                    } else if down && app_len > 0 {
                        self.focus = FocusTarget::App(0);
                    }
                } else if up {
                    self.focus = FocusTarget::Dock(dock_last);
                } else if down {
                    if has_favs {
                        self.focus = FocusTarget::Fav(0);
                    } else if app_len > 0 {
                        self.focus = FocusTarget::App(0);
                    }
                }
            }
            FocusTarget::Fav(i) => {
                if left {
                    self.focus = FocusTarget::Fav(i.saturating_sub(1));
                } else if right {
                    self.focus = FocusTarget::Fav((i + 1).min(favs.len().saturating_sub(1)));
                } else if up {
                    if has_weather {
                        self.focus = FocusTarget::Weather;
                    } else {
                        self.focus = FocusTarget::Dock(0);
                    }
                } else if down && app_len > 0 {
                    self.focus = FocusTarget::App(i.min(app_len - 1));
                }
            }
            FocusTarget::App(i) => {
                if is_zenith {
                    // Zenith Horizon: single sleek horizontal liquid-glass dock row
                    if left {
                        self.focus = FocusTarget::App(i.saturating_sub(1));
                    } else if right && i + 1 < app_len {
                        self.focus = FocusTarget::App(i + 1);
                    } else if up {
                        if has_weather {
                            self.focus = FocusTarget::Weather;
                        } else {
                            self.focus = FocusTarget::Dock(0);
                        }
                    } else if down {
                        self.last_app_idx = i;
                        self.focus = FocusTarget::RamPill;
                    }
                } else {
                    let col = i % cols;
                    let row = i / cols;
                    let total_rows = (app_len + cols - 1) / cols;
                    if left {
                        if col > 0 {
                            self.focus = FocusTarget::App(i - 1);
                        }
                    } else if right {
                        if col + 1 < cols && i + 1 < app_len {
                            self.focus = FocusTarget::App(i + 1);
                        }
                    } else if up {
                        if row > 0 {
                            self.focus = FocusTarget::App(i - cols);
                        } else if has_favs {
                            self.focus = FocusTarget::Fav(col.min(favs.len() - 1));
                        } else if has_weather {
                            self.focus = FocusTarget::Weather;
                        } else {
                            self.focus = FocusTarget::Dock(col.min(dock_last));
                        }
                    } else if down {
                        if row + 1 < total_rows {
                            let next = (i + cols).min(app_len - 1);
                            self.focus = FocusTarget::App(next);
                        } else {
                            self.last_app_idx = i;
                            self.focus = FocusTarget::RamPill;
                        }
                    }
                }
            }
            FocusTarget::RamPill => {
                if up {
                    if app_len > 0 {
                        self.focus = FocusTarget::App(self.last_app_idx.min(app_len - 1));
                    } else {
                        self.focus = FocusTarget::Dock(0);
                    }
                }
            }
        }

        if self.focus != prev {
            self.focus_changed = true;
            self.focus_changed_at = Instant::now();
        }
    }

    fn draw_focus_effects(
        &self,
        ctx: &egui::Context,
        p: &egui::Painter,
        center: Pos2,
        radius: f32,
        focused: bool,
        accent: Color32,
    ) {
        if !focused || self.config.lightweight_mode {
            return;
        }
        match self.config.focus_anim {
            FocusAnimStyle::KineticRipple => {
                let elapsed = self.focus_changed_at.elapsed().as_secs_f32();
                if elapsed < 0.36 {
                    let frac = (elapsed / 0.36).clamp(0.0, 1.0);
                    let ease = 1.0 - (1.0 - frac).powi(3);
                    let r = radius + 2.0 + ease * 18.0;
                    let a = ((1.0 - frac) * 210.0) as u8;
                    p.circle_stroke(
                        center,
                        r,
                        Stroke::new(
                            2.4_f32 * (1.0 - frac * 0.5),
                            Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), a),
                        ),
                    );
                    ctx.request_repaint();
                }
            }
            FocusAnimStyle::AmbientGlow => {
                // Soft 3-ring radial spotlight bloom behind focused icon (zero blur cost)
                for (mult, alpha) in [(1.55, 22u8), (1.34, 38u8), (1.16, 58u8)] {
                    p.circle_filled(
                        center,
                        radius * mult,
                        Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), alpha),
                    );
                }
            }
            FocusAnimStyle::BreathingHalo => {
                let t = self.start_time.elapsed().as_secs_f32();
                let wave = (t * 3.2).sin() * 0.5 + 0.5;
                let r = radius + 4.0 + wave * 5.0;
                let a = (70.0 + wave * 110.0) as u8;
                p.circle_stroke(
                    center,
                    r,
                    Stroke::new(
                        2.2_f32,
                        Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), a),
                    ),
                );
                // Native VSync pacing (60fps on 60Hz / 120fps on 120Hz displays)
                ctx.request_repaint();
            }
            FocusAnimStyle::MagneticWave | FocusAnimStyle::Minimal => {}
        }
    }

    fn resting_focus_motion(&self, focused: bool, focus_t: f32) -> (f32, f32) {
        if !focused
            || focus_t < 0.01
            || self.config.lightweight_mode
            || self.config.focus_anim == FocusAnimStyle::Minimal
        {
            return (0.0, 0.0);
        }
        let t = self.start_time.elapsed().as_secs_f32();
        let wave = (t * 2.1).sin();
        let float_y = wave * 2.4 * focus_t;
        let breath = (wave * 0.5 + 0.5) * focus_t;
        (float_y, breath)
    }

    fn draw_resting_focus_overlay(
        &self,
        ctx: &egui::Context,
        p: &egui::Painter,
        center: Pos2,
        radius: f32,
        focused: bool,
        focus_t: f32,
        accent: Color32,
    ) {
        if !focused
            || focus_t < 0.01
            || self.config.lightweight_mode
            || self.config.focus_anim == FocusAnimStyle::Minimal
        {
            return;
        }
        let t = self.start_time.elapsed().as_secs_f32();
        let base_ang = t * 1.45;
        let rim_r = radius + 0.4;

        // Primary liquid-glass specular comet arc orbiting the focused rim
        const SEGS: usize = 10;
        let sweep = 1.05_f32; // ~60 degrees
        for i in 0..SEGS {
            let f0 = i as f32 / SEGS as f32;
            let f1 = (i + 1) as f32 / SEGS as f32;
            let a0 = base_ang - sweep * (1.0 - f0);
            let a1 = base_ang - sweep * (1.0 - f1);
            let p0 = center + Vec2::new(a0.cos() * rim_r, a0.sin() * rim_r);
            let p1 = center + Vec2::new(a1.cos() * rim_r, a1.sin() * rim_r);
            // Smooth fade-in tail to crisp luminous specular head
            let intensity = (f1 * f1) * focus_t;
            let mix_white = f1.powf(1.6);
            let r = (accent.r() as f32 * (1.0 - mix_white) + 255.0 * mix_white) as u8;
            let g = (accent.g() as f32 * (1.0 - mix_white) + 255.0 * mix_white) as u8;
            let b = (accent.b() as f32 * (1.0 - mix_white) + 255.0 * mix_white) as u8;
            let alpha = (intensity * 235.0) as u8;
            let width = (1.2 + f1 * 2.2) * focus_t;
            p.line_segment(
                [p0, p1],
                Stroke::new(width, Color32::from_rgba_unmultiplied(r, g, b, alpha)),
            );
        }

        // Subtle opposing refractive glass glint on the opposite rim
        let opp_ang = base_ang + std::f32::consts::PI;
        let opp_sweep = 0.55_f32;
        for i in 0..5 {
            let f0 = i as f32 / 5.0;
            let f1 = (i + 1) as f32 / 5.0;
            let a0 = opp_ang - opp_sweep * (1.0 - f0);
            let a1 = opp_ang - opp_sweep * (1.0 - f1);
            let p0 = center + Vec2::new(a0.cos() * rim_r, a0.sin() * rim_r);
            let p1 = center + Vec2::new(a1.cos() * rim_r, a1.sin() * rim_r);
            let alpha = (f1 * 95.0 * focus_t) as u8;
            p.line_segment(
                [p0, p1],
                Stroke::new(1.6_f32 * focus_t, Color32::from_rgba_unmultiplied(255, 255, 255, alpha)),
            );
        }

        ctx.request_repaint();
    }

    fn draw_background(&self, ctx: &egui::Context, rect: Rect) {
        let painter = ctx.layer_painter(egui::LayerId::background());
        let scene = self.active_scene();

        let t = self.start_time.elapsed().as_secs_f32();
        let motion = self.config.live_sky_motion
            && !self.config.lightweight_mode
            && self.modal == ActiveModal::None;

        // Draw single opaque wallpaper pass when loaded (scrim is pre-baked off-thread for 3x faster GPU fill-rate!)
        if let Some((loaded_scene, _, tex)) = &self.wallpaper_tex {
            let tex_sz = tex.size_vec2();
            if tex_sz.x > 0.0 && tex_sz.y > 0.0 {
                if *loaded_scene == WeatherScene::LiveEarth {
                    // Pitch-black deep space backdrop with live Earth centered
                    painter.rect_filled(rect, 0.0, Color32::from_rgb(2, 4, 10));
                    let globe_side = rect.height() * 0.94;
                    let drift_x = if motion { (t * 0.07).sin() * 8.0 } else { 0.0 };
                    let globe_rect = Rect::from_center_size(
                        rect.center() + Vec2::new(drift_x, -6.0),
                        Vec2::splat(globe_side),
                    );
                    painter.image(
                        tex.id(),
                        globe_rect,
                        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                        Color32::WHITE,
                    );
                } else {
                    let screen_aspect = rect.width() / rect.height().max(1.0);
                    let img_aspect = tex_sz.x / tex_sz.y;
                    let mut uv = if img_aspect > screen_aspect {
                        let frac = screen_aspect / img_aspect;
                        let pad = (1.0 - frac) * 0.5;
                        Rect::from_min_max(Pos2::new(pad, 0.0), Pos2::new(1.0 - pad, 1.0))
                    } else {
                        let frac = img_aspect / screen_aspect;
                        let pad = (1.0 - frac) * 0.5;
                        Rect::from_min_max(Pos2::new(0.0, pad), Pos2::new(1.0, 1.0 - pad))
                    };
                    // Zero-cost real-time cinematic sky drift via gentle UV pan
                    if motion {
                        let inset = 0.022;
                        let dx = (t * 0.065).sin() * inset;
                        let dy = (t * 0.045).cos() * (inset * 0.6);
                        uv = Rect::from_min_max(
                            Pos2::new(
                                (uv.min.x + inset + dx).clamp(0.0, 0.45),
                                (uv.min.y + inset + dy).clamp(0.0, 0.45),
                            ),
                            Pos2::new(
                                (uv.max.x - inset + dx).clamp(0.55, 1.0),
                                (uv.max.y - inset + dy).clamp(0.55, 1.0),
                            ),
                        );
                    }
                    painter.image(tex.id(), rect, uv, Color32::WHITE);
                }
            }
        } else if scene == WeatherScene::AuroraBorealis {
            // Procedural GPU Aurora Borealis shader (zero network, silky 60/120Hz)
            painter.rect_filled(rect, 0.0, Color32::from_rgb(5, 10, 22));
            let bands = [
                (Color32::from_rgba_unmultiplied(16, 185, 129, 36), 0.28, 0.55, 1.4),
                (Color32::from_rgba_unmultiplied(56, 189, 248, 30), 0.42, 0.42, 2.1),
                (Color32::from_rgba_unmultiplied(168, 85, 247, 26), 0.54, 0.35, 3.0),
            ];
            for (col, y_frac, spd, phase) in bands {
                for step in 0..18 {
                    let xf = step as f32 / 17.0;
                    let x = rect.left() + xf * rect.width();
                    let wave = ((xf * 4.5 + t * spd + phase).sin() * 44.0)
                        + ((xf * 2.1 - t * spd * 0.7).cos() * 26.0);
                    let y = rect.top() + rect.height() * y_frac + wave;
                    painter.circle_filled(Pos2::new(x, y), 110.0, col);
                }
            }
            if motion {
                ctx.request_repaint();
            }
        } else if scene == WeatherScene::LiquidMesh {
            // Procedural GPU Liquid Gradient Mesh synced with active theme accent
            let acc = self.config.accent();
            let acc2 = self.config.accent2();
            painter.rect_filled(rect, 0.0, Color32::from_rgb(7, 10, 20));
            let orbs = [
                (0.24 + (t * 0.28).sin() * 0.14, 0.30 + (t * 0.22).cos() * 0.12, 260.0, acc, 42u8),
                (0.76 + (t * 0.24).cos() * 0.15, 0.38 + (t * 0.31).sin() * 0.14, 290.0, acc2, 38u8),
                (0.50 + (t * 0.19).sin() * 0.18, 0.72 + (t * 0.25).cos() * 0.10, 270.0, acc, 30u8),
            ];
            for (xf, yf, r, c, a) in orbs {
                let p = Pos2::new(rect.left() + rect.width() * xf, rect.top() + rect.height() * yf);
                painter.circle_filled(p, r, Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a));
                painter.circle_filled(
                    p,
                    r * 0.55,
                    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a.saturating_add(16)),
                );
            }
            if motion {
                ctx.request_repaint();
            }
        } else if scene == WeatherScene::DeepSpace {
            // Procedural GPU Deep Space Nebula + multi-depth twinkling starfield
            painter.rect_filled(rect, 0.0, Color32::from_rgb(4, 6, 15));
            painter.circle_filled(
                Pos2::new(rect.width() * 0.26, rect.height() * 0.30),
                240.0,
                Color32::from_rgba_unmultiplied(99, 102, 241, 24),
            );
            painter.circle_filled(
                Pos2::new(rect.width() * 0.74, rect.height() * 0.62),
                260.0,
                Color32::from_rgba_unmultiplied(236, 72, 153, 18),
            );
            for i in 0..72 {
                let fx = ((i * 73 + 19) % 1000) as f32 / 1000.0;
                let fy = ((i * 151 + 43) % 1000) as f32 / 1000.0;
                let tw = ((t * (1.2 + (i % 5) as f32 * 0.35) + i as f32).sin() * 0.5 + 0.5);
                let a = (70.0 + tw * 175.0) as u8;
                let r = if i % 7 == 0 { 1.8 } else { 1.1 };
                painter.circle_filled(
                    Pos2::new(rect.left() + fx * rect.width(), rect.top() + fy * rect.height()),
                    r,
                    Color32::from_rgba_unmultiplied(235, 244, 255, a),
                );
            }
            if motion {
                ctx.request_repaint();
            }
        } else {
            // Fallback atmospheric gradient while wallpaper downloads
            let (top_c, bot_c) = match scene {
                WeatherScene::ClearDay | WeatherScene::Auto | WeatherScene::LiveUkSat => {
                    (Color32::from_rgb(16, 46, 94), Color32::from_rgb(10, 18, 36))
                }
                WeatherScene::Golden => {
                    (Color32::from_rgb(82, 44, 62), Color32::from_rgb(18, 16, 30))
                }
                WeatherScene::ClearNight | WeatherScene::LiveEarth => {
                    (Color32::from_rgb(4, 8, 22), Color32::from_rgb(2, 4, 10))
                }
                WeatherScene::Cloudy | WeatherScene::Fog => {
                    (Color32::from_rgb(30, 38, 52), Color32::from_rgb(12, 16, 24))
                }
                WeatherScene::Rain => {
                    (Color32::from_rgb(18, 28, 46), Color32::from_rgb(8, 12, 22))
                }
                WeatherScene::Storm => {
                    (Color32::from_rgb(16, 16, 34), Color32::from_rgb(6, 8, 16))
                }
                WeatherScene::Snow => {
                    (Color32::from_rgb(28, 42, 66), Color32::from_rgb(12, 18, 30))
                }
                _ => (Color32::from_rgb(10, 16, 32), Color32::from_rgb(5, 8, 16)),
            };
            let mut mesh = egui::Mesh::default();
            mesh.colored_vertex(rect.left_top(), top_c);
            mesh.colored_vertex(rect.right_top(), top_c);
            mesh.colored_vertex(rect.right_bottom(), bot_c);
            mesh.colored_vertex(rect.left_bottom(), bot_c);
            mesh.add_triangle(0, 1, 2);
            mesh.add_triangle(0, 2, 3);
            painter.add(egui::Shape::mesh(mesh));
        }

        // Auto Night Dimming (22:00 - 06:29 London UK time, zero allocations)
        if self.config.auto_night_dim {
            let now = Local::now();
            let mins = now.hour() * 60 + now.minute();
            if mins >= 22 * 60 || mins < 6 * 60 + 30 {
                painter.rect_filled(
                    rect,
                    0.0,
                    Color32::from_rgba_unmultiplied(4, 6, 14, 72),
                );
            }
        }

        // Real-time GPU atmospheric effects (particles, drifting clouds, stars & shooting stars)
        match scene {
            WeatherScene::Rain | WeatherScene::Storm => {
                let count = if scene == WeatherScene::Storm { 70 } else { 45 };
                for idx in 0..count {
                    let seed = idx as f32 * 37.719;
                    let x = ((seed * 13.3 + t * 55.0) % (rect.width() + 80.0)) - 40.0;
                    let speed = 340.0 + (idx % 7) as f32 * 45.0;
                    let y = ((seed * 29.1 + t * speed) % (rect.height() + 60.0)) - 30.0;
                    let len = 14.0 + (idx % 4) as f32 * 5.0;
                    painter.line_segment(
                        [Pos2::new(x, y), Pos2::new(x - len * 0.22, y + len)],
                        Stroke::new(1.2_f32, Color32::from_rgba_unmultiplied(190, 225, 255, 85)),
                    );
                }
                if scene == WeatherScene::Storm {
                    let flash_phase = (t * 0.35) % 1.0;
                    if flash_phase > 0.965 {
                        painter.rect_filled(
                            rect,
                            0.0,
                            Color32::from_rgba_unmultiplied(220, 235, 255, 36),
                        );
                    }
                }
                ctx.request_repaint();
            }
            WeatherScene::Snow => {
                for idx in 0..55 {
                    let seed = idx as f32 * 53.17;
                    let drift = (t * 0.8 + seed).sin() * 24.0;
                    let x = ((seed * 17.9 + drift).rem_euclid(rect.width())).max(0.0);
                    let speed = 42.0 + (idx % 6) as f32 * 14.0;
                    let y = (seed * 23.3 + t * speed) % rect.height();
                    let r = 1.6 + (idx % 3) as f32 * 0.9;
                    painter.circle_filled(
                        Pos2::new(x, y),
                        r,
                        Color32::from_rgba_unmultiplied(240, 248, 255, 130),
                    );
                }
                ctx.request_repaint();
            }
            WeatherScene::ClearNight | WeatherScene::LiveEarth => {
                for idx in 0..56 {
                    let seed = idx as f32 * 71.13;
                    let x = (seed * 19.7) % rect.width();
                    let y = (seed * 11.3) % (rect.height() * if scene == WeatherScene::LiveEarth { 0.92 } else { 0.58 });
                    let twinkle = 0.4 + 0.6 * ((t * 1.4 + seed).sin() * 0.5 + 0.5);
                    let a = (twinkle * 175.0) as u8;
                    painter.circle_filled(
                        Pos2::new(x, y),
                        1.1 + (idx % 2) as f32 * 0.7,
                        Color32::from_rgba_unmultiplied(235, 244, 255, a),
                    );
                }
                // Occasional real-time shooting star every ~11s
                if motion {
                    let cycle = t % 11.0;
                    if cycle < 0.75 {
                        let p = cycle / 0.75;
                        let sx = rect.width() * (0.18 + p * 0.34);
                        let sy = rect.height() * (0.10 + p * 0.22);
                        let tail = Vec2::new(-54.0 * (1.0 - p * 0.4), -26.0 * (1.0 - p * 0.4));
                        let a = ((1.0 - p) * 200.0) as u8;
                        painter.line_segment(
                            [Pos2::new(sx, sy), Pos2::new(sx, sy) + tail],
                            Stroke::new(1.8_f32, Color32::from_rgba_unmultiplied(230, 245, 255, a)),
                        );
                    }
                    // Full native VSync (60Hz / 120Hz) for buttery shooting stars & sky drift
                    ctx.request_repaint();
                } else {
                    let subsec_ms = Local::now().timestamp_subsec_millis().min(999) as u64;
                    let wait_ms = (1000 - subsec_ms).max(16);
                    ctx.request_repaint_after(Duration::from_millis(wait_ms));
                }
            }
            _ => {
                // Smooth native-VSync cinematic UV sky drift when motion is on; 1Hz sleep when idle
                if motion {
                    ctx.request_repaint();
                } else {
                    let subsec_ms = Local::now().timestamp_subsec_millis().min(999) as u64;
                    let wait_ms = (1000 - subsec_ms).max(16);
                    ctx.request_repaint_after(Duration::from_millis(wait_ms));
                }
            }
        }
    }

    fn draw_weather_icon(p: &egui::Painter, c: Pos2, condition: &str, scale: f32) {
        let low = condition.to_lowercase();
        let sun_c = Color32::from_rgb(251, 191, 36);
        let cloud_c = Color32::from_rgb(226, 236, 248);
        let rain_c = Color32::from_rgb(56, 189, 248);

        if low.contains("clear") || low.contains("sun") {
            p.circle_filled(c, 5.0 * scale, sun_c);
            for k in 0..8 {
                let a = k as f32 * std::f32::consts::TAU / 8.0;
                p.line_segment(
                    [
                        c + Vec2::new(a.cos() * 7.0 * scale, a.sin() * 7.0 * scale),
                        c + Vec2::new(a.cos() * 9.5 * scale, a.sin() * 9.5 * scale),
                    ],
                    Stroke::new(1.5_f32 * scale, sun_c),
                );
            }
        } else if low.contains("partly") || low.contains("mainly") {
            p.circle_filled(c + Vec2::new(-3.2 * scale, -3.0 * scale), 4.0 * scale, sun_c);
            p.circle_filled(c + Vec2::new(1.5 * scale, 1.5 * scale), 4.5 * scale, cloud_c);
            p.circle_filled(c + Vec2::new(-2.5 * scale, 2.2 * scale), 3.6 * scale, cloud_c);
            p.circle_filled(c + Vec2::new(5.2 * scale, 2.5 * scale), 3.2 * scale, cloud_c);
        } else if low.contains("thunder") || low.contains("storm") {
            p.circle_filled(c + Vec2::new(-2.0 * scale, -2.5 * scale), 4.2 * scale, cloud_c);
            p.circle_filled(c + Vec2::new(3.0 * scale, -2.0 * scale), 3.6 * scale, cloud_c);
            p.add(egui::Shape::line(
                vec![
                    c + Vec2::new(0.5 * scale, 0.0),
                    c + Vec2::new(-2.0 * scale, 4.5 * scale),
                    c + Vec2::new(1.0 * scale, 4.5 * scale),
                    c + Vec2::new(-1.0 * scale, 8.8 * scale),
                ],
                Stroke::new(1.8_f32 * scale, sun_c),
            ));
        } else if low.contains("rain") || low.contains("drizzle") || low.contains("shower") {
            p.circle_filled(c + Vec2::new(-2.5 * scale, -2.5 * scale), 4.0 * scale, cloud_c);
            p.circle_filled(c + Vec2::new(2.8 * scale, -2.0 * scale), 3.5 * scale, cloud_c);
            for dx in [-3.5, 0.0, 3.5] {
                p.line_segment(
                    [
                        c + Vec2::new(dx * scale, 3.0 * scale),
                        c + Vec2::new((dx - 1.2) * scale, 7.2 * scale),
                    ],
                    Stroke::new(1.6_f32 * scale, rain_c),
                );
            }
        } else if low.contains("snow") {
            for k in 0..3 {
                let a = k as f32 * std::f32::consts::FRAC_PI_3;
                let v = Vec2::new(a.cos() * 6.5 * scale, a.sin() * 6.5 * scale);
                p.line_segment([c - v, c + v], Stroke::new(1.5_f32 * scale, Color32::WHITE));
            }
        } else if low.contains("fog") {
            for dy in [-3.5, 0.0, 3.5] {
                p.line_segment(
                    [
                        c + Vec2::new(-6.5 * scale, dy * scale),
                        c + Vec2::new(6.5 * scale, dy * scale),
                    ],
                    Stroke::new(1.7_f32 * scale, cloud_c),
                );
            }
        } else {
            // Overcast / Cloudy
            p.circle_filled(c + Vec2::new(-3.2 * scale, 0.5 * scale), 4.2 * scale, cloud_c);
            p.circle_filled(c + Vec2::new(1.2 * scale, -1.2 * scale), 5.0 * scale, cloud_c);
            p.circle_filled(c + Vec2::new(5.2 * scale, 1.0 * scale), 3.6 * scale, cloud_c);
        }
    }

    fn draw_action_icon(p: &egui::Painter, c: Pos2, id: &str, color: Color32) {
        let s = Stroke::new(1.6_f32, color);
        match id {
            "open" => {
                p.add(egui::Shape::convex_polygon(
                    vec![
                        c + Vec2::new(-3.5, -5.0),
                        c + Vec2::new(5.0, 0.0),
                        c + Vec2::new(-3.5, 5.0),
                    ],
                    color,
                    Stroke::NONE,
                ));
            }
            "pin" => {
                let mut pts = Vec::new();
                for k in 0..10 {
                    let r = if k % 2 == 0 { 6.0 } else { 2.6 };
                    let a = -std::f32::consts::FRAC_PI_2 + k as f32 * std::f32::consts::TAU / 10.0;
                    pts.push(c + Vec2::new(a.cos() * r, a.sin() * r));
                }
                p.add(egui::Shape::closed_line(pts, s));
            }
            "left" => {
                p.line_segment([c + Vec2::new(4.5, 0.0), c + Vec2::new(-4.5, 0.0)], s);
                p.add(egui::Shape::line(
                    vec![c + Vec2::new(-1.0, -4.0), c + Vec2::new(-4.8, 0.0), c + Vec2::new(-1.0, 4.0)],
                    s,
                ));
            }
            "right" => {
                p.line_segment([c + Vec2::new(-4.5, 0.0), c + Vec2::new(4.5, 0.0)], s);
                p.add(egui::Shape::line(
                    vec![c + Vec2::new(1.0, -4.0), c + Vec2::new(4.8, 0.0), c + Vec2::new(1.0, 4.0)],
                    s,
                ));
            }
            "rename" => {
                p.line_segment([c + Vec2::new(-4.5, 4.5), c + Vec2::new(4.0, -4.0)], Stroke::new(2.0_f32, color));
                p.line_segment([c + Vec2::new(-5.0, 5.0), c + Vec2::new(-2.0, 5.0)], s);
            }
            "hide" => Self::draw_setting_icon(p, c, 12, color),
            "info" => {
                p.circle_stroke(c, 5.8, s);
                p.circle_filled(c + Vec2::new(0.0, -2.4), 1.1, color);
                p.line_segment([c + Vec2::new(0.0, -0.3), c + Vec2::new(0.0, 3.2)], Stroke::new(1.8_f32, color));
            }
            "uninstall" => {
                p.line_segment([c + Vec2::new(-5.0, -3.5), c + Vec2::new(5.0, -3.5)], s);
                p.rect_stroke(
                    Rect::from_center_size(c + Vec2::new(0.0, 1.2), Vec2::new(7.5, 8.2)),
                    CornerRadius::same(1),
                    s,
                    egui::StrokeKind::Middle,
                );
                p.line_segment([c + Vec2::new(-2.0, -5.2), c + Vec2::new(2.0, -5.2)], s);
            }
            _ => {
                p.circle_stroke(c, 4.5, s);
            }
        }
    }

    fn draw_theme_icon(p: &egui::Painter, c: Pos2, row: usize, color: Color32, accent: Color32) {
        let s = Stroke::new(1.5_f32, color);
        match row {
            // Astryx Theme: 4-swatch grid
            0 => Self::draw_dock_icon(p, c, 3, color),
            // Live Weather Wallpaper Scene: sun + rays
            1 => Self::draw_setting_icon(p, c, 4, color),
            // Accent Color: glowing accent orb with ring
            2 => {
                p.circle_filled(c, 4.5, accent);
                p.circle_stroke(c, 6.2, Stroke::new(1.3_f32, color));
            }
            // 60fps Focus Animation: concentric ripple rings
            3 => {
                p.circle_filled(c, 2.0, color);
                p.circle_stroke(c, 4.5, Stroke::new(1.2_f32, color));
                p.circle_stroke(
                    c,
                    6.8,
                    Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 150)),
                );
            }
            // Modal Pop-In Animation: layered pop-up frames
            4 => {
                p.rect_stroke(
                    Rect::from_center_size(c + Vec2::new(-1.5, 1.5), Vec2::new(8.5, 6.5)),
                    CornerRadius::same(2),
                    Stroke::new(1.2_f32, Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 140)),
                    egui::StrokeKind::Middle,
                );
                p.rect_stroke(
                    Rect::from_center_size(c + Vec2::new(1.5, -1.5), Vec2::new(8.5, 6.5)),
                    CornerRadius::same(2),
                    s,
                    egui::StrokeKind::Middle,
                );
            }
            // Menu Transparency: overlapping glass circles
            5 => {
                p.circle_stroke(c + Vec2::new(-2.4, 0.0), 4.4, s);
                p.circle_filled(
                    c + Vec2::new(2.4, 0.0),
                    4.4,
                    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 90),
                );
                p.circle_stroke(c + Vec2::new(2.4, 0.0), 4.4, s);
            }
            // Menu Opacity: contrast dial
            6 => {
                p.circle_stroke(c, 5.6, s);
                p.rect_filled(
                    Rect::from_min_max(c + Vec2::new(0.0, -4.6), c + Vec2::new(4.8, 4.6)),
                    CornerRadius::same(2),
                    color,
                );
            }
            // Background Blur: soft bokeh dots
            7 => {
                p.circle_filled(c + Vec2::new(-3.0, -2.0), 2.8, Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 170));
                p.circle_filled(c + Vec2::new(3.2, -1.2), 2.2, Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 130));
                p.circle_filled(c + Vec2::new(0.2, 3.2), 2.6, Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 210));
            }
            // Real-Time Sky Motion: flowing wind/sky waves
            _ => {
                p.line_segment([c + Vec2::new(-5.5, -2.8), c + Vec2::new(3.5, -2.8)], s);
                p.line_segment([c + Vec2::new(-3.5, 0.2), c + Vec2::new(5.5, 0.2)], s);
                p.line_segment([c + Vec2::new(-5.0, 3.2), c + Vec2::new(2.5, 3.2)], s);
            }
        }
    }

    fn setting_icon_color(kind: usize) -> Color32 {
        match kind {
            0 => Color32::from_rgb(56, 189, 248),   // Layout: Sky Cyan
            1 => Color32::from_rgb(168, 85, 247),   // Icon Size: Violet
            2 => Color32::from_rgb(129, 140, 248),  // Sort: Indigo
            3 | 6 => Color32::from_rgb(96, 165, 250), // Clock: Electric Blue
            4 => Color32::from_rgb(251, 191, 36),   // Weather / City: Sun Gold
            5 => Color32::from_rgb(244, 114, 182),  // Labels: Rose Pink
            7 => Color32::from_rgb(52, 211, 153),   // Status / Now Playing: Emerald
            8 => Color32::from_rgb(192, 132, 252),  // Night Dim / Ambient: Lavender
            9 => Color32::from_rgb(45, 212, 191),   // Mobile Apps: Teal
            10 => Color32::from_rgb(148, 163, 184), // System Apps: Cool Silver
            11 => Color32::from_rgb(74, 222, 128),  // Default Home: Mint Green
            12 => Color32::from_rgb(251, 113, 133), // Hidden Apps: Coral
            13 => Color32::from_rgb(56, 189, 248),  // App Updates: Bright Sky
            14 => Color32::from_rgb(52, 211, 153),  // HUD Overlay: Neon Emerald
            15 => Color32::from_rgb(255, 221, 0),   // Buy Me a Coffee: Coffee Gold
            100 | 202 => Color32::from_rgb(251, 146, 60), // Power / Display / GPU: Amber Orange
            101 => Color32::from_rgb(34, 211, 238), // RAM / CPU: Cyan
            102 => Color32::from_rgb(52, 211, 153), // Backup / Storage: Emerald
            200 => Color32::from_rgb(56, 189, 248), // Phone Companion / Network: Sky Blue
            201 => Color32::from_rgb(192, 132, 252), // Folders / Settings View: Purple
            _ => Color32::from_rgb(125, 211, 252),
        }
    }

    fn theme_icon_color(idx: usize, accent: Color32) -> Color32 {
        match idx {
            0 => Color32::from_rgb(168, 85, 247),  // Astryx Theme: Violet
            1 => Color32::from_rgb(251, 191, 36),  // Live Wallpaper: Sun Gold
            2 => accent,                           // Accent Color: Active Accent
            3 => Color32::from_rgb(34, 211, 238),  // Focus Animation: Electric Cyan
            4 => Color32::from_rgb(244, 114, 182), // Modal Pop-In: Rose Pink
            5 => Color32::from_rgb(56, 189, 248),  // Live Sky Motion: Sky Blue
            6 => Color32::from_rgb(45, 212, 191),  // Menu Transparency: Teal
            7 => Color32::from_rgb(129, 140, 248), // Glass Opacity: Indigo
            _ => Color32::from_rgb(192, 132, 252), // Backdrop Blur: Lavender
        }
    }

    fn draw_setting_icon(p: &egui::Painter, c: Pos2, kind: usize, color: Color32) {
        let s = Stroke::new(1.5_f32, color);
        match kind {
            // Layout Style: horizontal bar + bottom dock
            0 => {
                p.rect_stroke(Rect::from_center_size(c, Vec2::new(12.0, 9.0)), CornerRadius::same(2), s, egui::StrokeKind::Middle);
                p.line_segment([c + Vec2::new(-3.5, 2.2), c + Vec2::new(3.5, 2.2)], s);
            }
            // Icon Size: nested circles
            1 => {
                p.circle_stroke(c, 5.5, s);
                p.circle_filled(c, 2.2, color);
            }
            // Sort Order: 3 staggered horizontal bars
            2 => {
                p.line_segment([c + Vec2::new(-5.0, -3.5), c + Vec2::new(5.0, -3.5)], s);
                p.line_segment([c + Vec2::new(-5.0, 0.0), c + Vec2::new(2.5, 0.0)], s);
                p.line_segment([c + Vec2::new(-5.0, 3.5), c + Vec2::new(0.0, 3.5)], s);
            }
            // Clock / 24h Clock: circle + hands
            3 | 6 => {
                p.circle_stroke(c, 5.5, s);
                p.line_segment([c, c + Vec2::new(0.0, -3.2)], s);
                p.line_segment([c, c + Vec2::new(2.6, 1.0)], s);
            }
            // Weather: sun + rays
            4 => {
                p.circle_stroke(c, 3.0, s);
                for k in 0..8 {
                    let a = k as f32 * std::f32::consts::TAU / 8.0;
                    p.line_segment(
                        [c + Vec2::new(a.cos() * 4.5, a.sin() * 4.5), c + Vec2::new(a.cos() * 6.2, a.sin() * 6.2)],
                        Stroke::new(1.3_f32, color),
                    );
                }
            }
            // App Labels: "T" typography mark
            5 => {
                p.line_segment([c + Vec2::new(-4.5, -4.0), c + Vec2::new(4.5, -4.0)], Stroke::new(1.7_f32, color));
                p.line_segment([c + Vec2::new(0.0, -4.0), c + Vec2::new(0.0, 4.8)], Stroke::new(1.7_f32, color));
            }
            // Live Status Pill: signal bars
            7 => {
                p.line_segment([c + Vec2::new(-4.0, 4.0), c + Vec2::new(-4.0, 1.0)], Stroke::new(1.8_f32, color));
                p.line_segment([c + Vec2::new(0.0, 4.0), c + Vec2::new(0.0, -1.5)], Stroke::new(1.8_f32, color));
                p.line_segment([c + Vec2::new(4.0, 4.0), c + Vec2::new(4.0, -4.0)], Stroke::new(1.8_f32, color));
            }
            // Auto Night Dimming: crescent moon
            8 => {
                let mut pts = Vec::new();
                for step in 0..=12 {
                    let ang = -0.8 + (step as f32 / 12.0) * 4.2;
                    pts.push(c + Vec2::new(ang.cos() * 5.0, ang.sin() * 5.0));
                }
                p.add(egui::Shape::line(pts, Stroke::new(1.7_f32, color)));
            }
            // Mobile Apps: phone outline
            9 => {
                p.rect_stroke(Rect::from_center_size(c, Vec2::new(7.5, 11.5)), CornerRadius::same(2), s, egui::StrokeKind::Middle);
                p.circle_filled(c + Vec2::new(0.0, 3.6), 1.0, color);
            }
            // System Apps / RAM chip: square + pins
            10 | 101 => {
                p.rect_stroke(Rect::from_center_size(c, Vec2::splat(7.2)), CornerRadius::same(1), s, egui::StrokeKind::Middle);
                for d in [-2.0, 2.0] {
                    p.line_segment([c + Vec2::new(d, -3.8), c + Vec2::new(d, -5.8)], s);
                    p.line_segment([c + Vec2::new(d, 3.8), c + Vec2::new(d, 5.8)], s);
                    p.line_segment([c + Vec2::new(-3.8, d), c + Vec2::new(-5.8, d)], s);
                    p.line_segment([c + Vec2::new(3.8, d), c + Vec2::new(5.8, d)], s);
                }
            }
            // Default Home Launcher: house
            11 => {
                p.add(egui::Shape::line(
                    vec![c + Vec2::new(-5.5, 0.0), c + Vec2::new(0.0, -5.2), c + Vec2::new(5.5, 0.0)],
                    s,
                ));
                p.rect_stroke(Rect::from_center_size(c + Vec2::new(0.0, 2.4), Vec2::new(7.4, 5.2)), CornerRadius::same(1), s, egui::StrokeKind::Middle);
            }
            // Hidden Apps: eye outline
            12 => {
                p.add(egui::Shape::line(
                    vec![c + Vec2::new(-5.8, 0.0), c + Vec2::new(0.0, -3.8), c + Vec2::new(5.8, 0.0)],
                    s,
                ));
                p.add(egui::Shape::line(
                    vec![c + Vec2::new(-5.8, 0.0), c + Vec2::new(0.0, 3.8), c + Vec2::new(5.8, 0.0)],
                    s,
                ));
                p.circle_filled(c, 1.8, color);
            }
            // App Updates: crisp downward arrow into tray
            13 => {
                p.line_segment([c + Vec2::new(0.0, -5.2), c + Vec2::new(0.0, 1.8)], Stroke::new(1.7_f32, color));
                p.add(egui::Shape::line(
                    vec![c + Vec2::new(-3.2, -0.8), c + Vec2::new(0.0, 2.2), c + Vec2::new(3.2, -0.8)],
                    Stroke::new(1.7_f32, color),
                ));
                p.add(egui::Shape::line(
                    vec![
                        c + Vec2::new(-5.0, 2.8),
                        c + Vec2::new(-5.0, 5.0),
                        c + Vec2::new(5.0, 5.0),
                        c + Vec2::new(5.0, 2.8),
                    ],
                    s,
                ));
            }
            // FPS Meter: speedometer gauge arc + needle
            14 => {
                let mut pts = Vec::with_capacity(11);
                for step in 0..=10 {
                    let ang = std::f32::consts::PI * 0.85 + (step as f32 / 10.0) * std::f32::consts::PI * 1.3;
                    pts.push(c + Vec2::new(ang.cos() * 5.4, ang.sin() * 5.4));
                }
                p.add(egui::Shape::line(pts, s));
                p.line_segment([c + Vec2::new(0.0, 1.2), c + Vec2::new(3.6, -2.8)], Stroke::new(1.7_f32, color));
                p.circle_filled(c + Vec2::new(0.0, 1.2), 1.4, color);
            }
            // Buy Me a Coffee: coffee cup + handle + saucer
            15 => {
                p.rect_stroke(
                    Rect::from_center_size(c + Vec2::new(-1.0, 0.2), Vec2::new(7.4, 6.8)),
                    CornerRadius::same(2),
                    s,
                    egui::StrokeKind::Middle,
                );
                // Cup handle
                p.rect_stroke(
                    Rect::from_center_size(c + Vec2::new(4.0, 0.0), Vec2::new(2.8, 3.8)),
                    CornerRadius::same(2),
                    s,
                    egui::StrokeKind::Middle,
                );
                // Saucer + steam
                p.line_segment([c + Vec2::new(-5.2, 5.0), c + Vec2::new(4.2, 5.0)], s);
                p.line_segment([c + Vec2::new(-2.2, -4.4), c + Vec2::new(-2.2, -5.8)], Stroke::new(1.3_f32, color));
                p.line_segment([c + Vec2::new(0.6, -4.4), c + Vec2::new(0.6, -5.8)], Stroke::new(1.3_f32, color));
            }
            // Device / Display: TV monitor
            100 | 202 => {
                p.rect_stroke(Rect::from_center_size(c + Vec2::new(0.0, -1.0), Vec2::new(11.5, 7.6)), CornerRadius::same(2), s, egui::StrokeKind::Middle);
                p.line_segment([c + Vec2::new(-3.2, 4.8), c + Vec2::new(3.2, 4.8)], s);
            }
            // Storage drive: cylinder/box
            102 => {
                p.rect_stroke(Rect::from_center_size(c, Vec2::new(11.0, 7.5)), CornerRadius::same(2), s, egui::StrokeKind::Middle);
                p.circle_filled(c + Vec2::new(3.0, 0.0), 1.2, color);
                p.line_segment([c + Vec2::new(-3.5, 0.0), c + Vec2::new(0.5, 0.0)], s);
            }
            // Network shortcut: Wi-Fi
            200 => Self::draw_dock_icon(p, c, 0, color),
            // Apps shortcut: 4-square grid
            201 => Self::draw_dock_icon(p, c, 3, color),
            // Gemini AI sparkle shortcut
            203 => Self::draw_dock_icon(p, c, 7, color),
            // System Settings shortcut: cog
            _ => Self::draw_dock_icon(p, c, 6, color),
        }
    }

    fn dock_items(&self) -> Vec<(usize, &'static str)> {
        if self.config.gemini_enabled {
            vec![
                (0, "Wi-Fi"),
                (1, "VPN"),
                (2, "Search"),
                (7, "Gemini AI"),
                (3, "Theme"),
                (4, "A2UI"),
                (5, "Refresh"),
                (6, "Settings"),
            ]
        } else {
            vec![
                (0, "Wi-Fi"),
                (1, "VPN"),
                (2, "Search"),
                (3, "Theme"),
                (4, "A2UI"),
                (5, "Refresh"),
                (6, "Settings"),
            ]
        }
    }

    fn trigger_dock_action(&mut self, action_id: usize, ctx: &egui::Context) {
        match action_id {
            0 => bridge::open_system_settings("network"),
            1 => self.open_vpn_app(),
            2 => self.open_modal(ActiveModal::Search),
            3 => self.open_modal(ActiveModal::Theme),
            4 => self.open_modal(ActiveModal::A2ui),
            5 => self.force_refresh_all(ctx),
            6 => self.open_modal(ActiveModal::Settings),
            7 => {
                self.gemini_text_mode = false;
                self.open_modal(ActiveModal::Gemini);
                if !self.config.gemini_api_key.trim().is_empty() && !self.gemini_busy {
                    self.start_gemini_voice_listen();
                }
            }
            _ => {}
        }
    }

    fn draw_gemini_sparkle(painter: &egui::Painter, c: Pos2, radius: f32, color: Color32) {
        let waist = radius * 0.27;
        let vert = vec![
            c + Vec2::new(0.0, -radius),
            c + Vec2::new(waist, 0.0),
            c + Vec2::new(0.0, radius),
            c + Vec2::new(-waist, 0.0),
        ];
        let horiz = vec![
            c + Vec2::new(-radius, 0.0),
            c + Vec2::new(0.0, -waist),
            c + Vec2::new(radius, 0.0),
            c + Vec2::new(0.0, waist),
        ];
        painter.add(egui::Shape::convex_polygon(vert, color, Stroke::NONE));
        painter.add(egui::Shape::convex_polygon(horiz, color, Stroke::NONE));
        painter.circle_filled(c, waist * 1.05, color);
    }

    fn draw_gemini_voice_orb(
        &self,
        ui: &mut Ui,
        ctx: &egui::Context,
        listening: bool,
        thinking: bool,
        speaking: bool,
        audio_level: f32,
    ) -> egui::Response {
        let t = self.start_time.elapsed().as_secs_f32();
        let accent = self.config.accent();
        let accent2 = self.config.accent2();
        let live_c = if listening {
            Color32::from_rgb(52, 211, 153) // Emerald-cyan live mic
        } else if thinking {
            Color32::from_rgb(168, 85, 247) // Violet reasoning vortex
        } else if speaking {
            accent
        } else {
            accent
        };

        let energy = if listening {
            0.30 + audio_level.clamp(0.0, 1.0) * 0.85
        } else if speaking {
            0.28 + audio_level.clamp(0.0, 1.0) * 0.80
        } else if thinking {
            0.55 + (t * 4.2).sin() * 0.15
        } else {
            0.14 + (t * 1.6).sin() * 0.04
        };

        let area_size = Vec2::new(ui.available_width(), 206.0);
        let (rect, resp) = ui.allocate_exact_size(area_size, Sense::click());
        let p = ui.painter();
        let center = rect.center();
        let base_r = 56.0_f32;

        // 1. Expanding Sonar / Atmospheric Aura Rings
        for ring_i in 0..3 {
            let phase = ((t * (if listening || speaking { 0.85 } else { 0.42 })
                + ring_i as f32 * 0.33)
                % 1.0)
                .clamp(0.0, 1.0);
            let r = base_r + 12.0 + phase * (38.0 + energy * 24.0);
            let alpha = ((1.0 - phase) * (42.0 + energy * 52.0)).clamp(0.0, 110.0) as u8;
            let rc = if ring_i % 2 == 0 { live_c } else { accent2 };
            p.circle_stroke(
                center,
                r,
                Stroke::new(
                    1.4_f32,
                    Color32::from_rgba_unmultiplied(rc.r(), rc.g(), rc.b(), alpha),
                ),
            );
        }

        // 2. 28-Bar Radial Soundwave Equalizer (reacts live to mic / TTS voice level!)
        let bar_count = 28usize;
        for b in 0..bar_count {
            let ang = (b as f32 / bar_count as f32) * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
            let dir = Vec2::new(ang.cos(), ang.sin());
            let wave = ((ang * 4.0 + t * 7.5).sin() * 0.5 + 0.5)
                * ((ang * 3.0 - t * 5.2).cos() * 0.5 + 0.5);
            let bar_len = if listening || speaking {
                4.0 + (audio_level * 26.0 + 5.0) * (0.35 + wave * 0.95)
            } else if thinking {
                4.0 + 10.0 * ((ang * 2.0 + t * 6.5).sin() * 0.5 + 0.5)
            } else {
                2.5 + 2.5 * ((ang * 3.0 + t * 1.8).sin() * 0.5 + 0.5)
            };
            let inner = center + dir * (base_r + 14.0);
            let outer = center + dir * (base_r + 14.0 + bar_len);
            let bar_alpha = if listening || speaking {
                (115.0 + audio_level * 135.0).clamp(90.0, 250.0) as u8
            } else if thinking {
                145
            } else {
                68
            };
            let bc = if b % 2 == 0 { live_c } else { accent2 };
            p.line_segment(
                [inner, outer],
                Stroke::new(
                    2.6_f32,
                    Color32::from_rgba_unmultiplied(bc.r(), bc.g(), bc.b(), bar_alpha),
                ),
            );
        }

        // 3. Deep Translucent Core Sphere
        p.circle_filled(
            center,
            base_r - 2.0,
            Color32::from_rgba_unmultiplied(10, 15, 30, 225),
        );
        p.circle_filled(
            center,
            base_r * (0.68 + energy * 0.14),
            Color32::from_rgba_unmultiplied(live_c.r(), live_c.g(), live_c.b(), 36),
        );

        // 4. Three Phase-Shifted 64-Vertex Harmonic Liquid-Plasma Ribbons
        let speed = if thinking {
            4.8
        } else if listening || speaking {
            3.6
        } else {
            1.4
        };
        let layers: [(Color32, f32, f32, f32); 3] = [
            (
                Color32::from_rgba_unmultiplied(accent2.r(), accent2.g(), accent2.b(), 185),
                2.4_f32,
                1.0,
                0.0,
            ),
            (
                Color32::from_rgba_unmultiplied(live_c.r(), live_c.g(), live_c.b(), 235),
                2.8_f32,
                -1.35,
                1.1,
            ),
            (
                Color32::from_rgba_unmultiplied(235, 246, 255, 195),
                1.5_f32,
                1.75,
                2.3,
            ),
        ];
        let segs = 64usize;
        let amp = 2.2 + energy * 11.5;
        for (col, width, dir_mul, phase_off) in layers {
            let mut pts = Vec::with_capacity(segs);
            for k in 0..segs {
                let th = (k as f32 / segs as f32) * std::f32::consts::TAU;
                let h1 = (th * 3.0 + t * speed * dir_mul + phase_off).sin();
                let h2 = (th * 5.0 - t * speed * 1.3 * dir_mul + phase_off * 1.7).cos() * 0.6;
                let h3 = (th * 7.0 + t * speed * 0.8 * dir_mul).sin() * 0.32;
                let r = base_r + amp * (h1 + h2 + h3) * 0.55;
                pts.push(center + Vec2::new(th.cos() * r, th.sin() * r));
            }
            p.add(egui::Shape::closed_line(pts, Stroke::new(width, col)));
        }

        // 5. Twin Orbiting Gemini 4-Point Sparkle Satellites
        let orbit_speed = if thinking { 3.4 } else { 1.15 };
        for sat in 0..2 {
            let a = t * orbit_speed + sat as f32 * std::f32::consts::PI;
            let orbit_r = base_r + 9.0 + (a * 2.0).sin() * 4.0;
            let pos = center + Vec2::new(a.cos() * orbit_r, a.sin() * orbit_r * 0.88);
            let sc = if sat == 0 {
                Color32::WHITE
            } else {
                accent2
            };
            Self::draw_gemini_sparkle(p, pos, if thinking { 6.2 } else { 4.8 }, sc);
        }

        // 6. Central Vector State Glyph (Mic / Thinking Sparkle / Speaking Waves)
        if listening {
            // Glowing Microphone capsule + stand
            let mic_c = Color32::WHITE;
            p.rect_filled(
                Rect::from_center_size(center + Vec2::new(0.0, -4.5), Vec2::new(10.0, 16.0)),
                CornerRadius::same(5),
                mic_c,
            );
            let mut arc_pts = Vec::new();
            for step in 0..=12 {
                let a = (step as f32 / 12.0) * std::f32::consts::PI;
                arc_pts.push(center + Vec2::new(a.cos() * 9.0, a.sin() * 9.0 - 3.5));
            }
            p.add(egui::Shape::line(arc_pts, Stroke::new(2.2_f32, mic_c)));
            p.line_segment(
                [center + Vec2::new(0.0, 5.5), center + Vec2::new(0.0, 11.0)],
                Stroke::new(2.2_f32, mic_c),
            );
            p.line_segment(
                [center + Vec2::new(-5.0, 11.0), center + Vec2::new(5.0, 11.0)],
                Stroke::new(2.2_f32, mic_c),
            );
        } else if speaking {
            // Animated 5-bar inner voice wave
            for k in -2i32..=2 {
                let x = center.x + k as f32 * 6.2;
                let h = 6.0
                    + (1.0 - (k.abs() as f32 * 0.28))
                        * (8.0 + audio_level * 20.0 * ((t * 10.0 + k as f32).sin() * 0.5 + 0.5));
                p.line_segment(
                    [Pos2::new(x, center.y - h), Pos2::new(x, center.y + h)],
                    Stroke::new(3.0_f32, Color32::WHITE),
                );
            }
        } else {
            let pulse = 12.5 + (t * if thinking { 5.0 } else { 2.0 }).sin() * 2.2;
            Self::draw_gemini_sparkle(p, center, pulse, Color32::WHITE);
            Self::draw_gemini_sparkle(
                p,
                center + Vec2::new(11.5, -11.5),
                4.8,
                live_c,
            );
        }

        ctx.request_repaint();
        resp
    }

    fn draw_dock_icon(painter: &egui::Painter, center: Pos2, idx: usize, color: Color32) {
        let s = Stroke::new(1.8, color);
        match idx {
            0 => {
                // Wi-Fi arcs
                painter.circle_filled(center + Vec2::new(0.0, 5.5), 1.8, color);
                for r in [5.5, 9.5] {
                    let mut pts = Vec::new();
                    for step in 0..=8 {
                        let ang = -std::f32::consts::FRAC_PI_4 * 3.0
                            + (step as f32 / 8.0) * std::f32::consts::FRAC_PI_2;
                        pts.push(center + Vec2::new(ang.cos() * r, ang.sin() * r + 5.5));
                    }
                    painter.add(egui::Shape::line(pts, s));
                }
            }
            1 => {
                // VPN shield (subtle tinted fill + bold outline for clear green/red visibility)
                let pts = vec![
                    center + Vec2::new(0.0, -7.5),
                    center + Vec2::new(6.5, -4.5),
                    center + Vec2::new(5.5, 2.5),
                    center + Vec2::new(0.0, 7.5),
                    center + Vec2::new(-5.5, 2.5),
                    center + Vec2::new(-6.5, -4.5),
                ];
                painter.add(egui::Shape::convex_polygon(
                    pts.clone(),
                    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 72),
                    Stroke::NONE,
                ));
                painter.add(egui::Shape::closed_line(pts, Stroke::new(2.0_f32, color)));
            }
            2 => {
                // Search magnifier
                painter.circle_stroke(center + Vec2::new(-1.5, -1.5), 5.2, s);
                painter.line_segment(
                    [center + Vec2::new(2.3, 2.3), center + Vec2::new(6.8, 6.8)],
                    Stroke::new(2.0, color),
                );
            }
            3 => {
                // Theme 4-swatch diamond grid (clean, modern palette)
                let offsets = [
                    Vec2::new(-3.4, -3.4),
                    Vec2::new(3.4, -3.4),
                    Vec2::new(-3.4, 3.4),
                    Vec2::new(3.4, 3.4),
                ];
                for off in offsets {
                    painter.rect_filled(
                        Rect::from_center_size(center + off, Vec2::splat(4.6)),
                        1.5,
                        color,
                    );
                }
            }
            4 => {
                // A2UI stacked diamond layers
                let top = vec![
                    center + Vec2::new(0.0, -7.0),
                    center + Vec2::new(7.0, -2.5),
                    center + Vec2::new(0.0, 2.0),
                    center + Vec2::new(-7.0, -2.5),
                ];
                painter.add(egui::Shape::closed_line(top, s));
                painter.add(egui::Shape::line(
                    vec![
                        center + Vec2::new(-6.5, 2.5),
                        center + Vec2::new(0.0, 6.5),
                        center + Vec2::new(6.5, 2.5),
                    ],
                    s,
                ));
            }
            5 => {
                // Refresh circular arrow
                let mut pts = Vec::new();
                for step in 0..=14 {
                    let ang = -0.5 + (step as f32 / 14.0) * 5.0;
                    pts.push(center + Vec2::new(ang.cos() * 6.0, ang.sin() * 6.0));
                }
                painter.add(egui::Shape::line(pts, s));
                let tip = center + Vec2::new((-0.5f32).cos() * 6.0, (-0.5f32).sin() * 6.0);
                painter.line_segment([tip, tip + Vec2::new(-3.8, -0.5)], s);
                painter.line_segment([tip, tip + Vec2::new(0.5, -3.8)], s);
            }
            7 => {
                // Official Google Gemini 4-point concave Bézier star with smooth GPU vertex gradient
                let r = 10.5_f32;
                let ctrl = 0.85_f32;
                let quads: [(Vec2, Vec2, Vec2); 4] = [
                    (Vec2::new(0.0, -r), Vec2::new(ctrl, -ctrl), Vec2::new(r, 0.0)),
                    (Vec2::new(r, 0.0), Vec2::new(ctrl, ctrl), Vec2::new(0.0, r)),
                    (Vec2::new(0.0, r), Vec2::new(-ctrl, ctrl), Vec2::new(-r, 0.0)),
                    (Vec2::new(-r, 0.0), Vec2::new(-ctrl, -ctrl), Vec2::new(0.0, -r)),
                ];

                // Smooth Gemini brand gradient: Cyan-Blue (#1BA1E3) -> Blue (#4E86F5) -> Violet (#9B72CB) -> Rose (#D96570)
                let grad_color = |off: Vec2| -> Color32 {
                    let u = ((off.x + off.y) / (2.0 * r) + 0.5).clamp(0.0, 1.0);
                    let stops: [(f32, [f32; 3]); 4] = [
                        (0.00, [27.0, 161.0, 227.0]),  // #1BA1E3
                        (0.38, [78.0, 134.0, 245.0]),  // #4E86F5
                        (0.72, [155.0, 114.0, 203.0]), // #9B72CB
                        (1.00, [217.0, 101.0, 112.0]), // #D96570
                    ];
                    let mut rgb = stops[3].1;
                    for w in stops.windows(2) {
                        if u >= w[0].0 && u <= w[1].0 {
                            let f = ((u - w[0].0) / (w[1].0 - w[0].0)).clamp(0.0, 1.0);
                            rgb = [
                                w[0].1[0] + (w[1].1[0] - w[0].1[0]) * f,
                                w[0].1[1] + (w[1].1[1] - w[0].1[1]) * f,
                                w[0].1[2] + (w[1].1[2] - w[0].1[2]) * f,
                            ];
                            break;
                        }
                    }
                    Color32::from_rgb(rgb[0] as u8, rgb[1] as u8, rgb[2] as u8)
                };

                let mut ring: Vec<Vec2> = Vec::with_capacity(40);
                let steps = 10usize;
                for (p0, p1, p2) in quads {
                    for s_i in 0..steps {
                        let t = s_i as f32 / steps as f32;
                        let inv = 1.0 - t;
                        let pt = p0 * (inv * inv) + p1 * (2.0 * inv * t) + p2 * (t * t);
                        ring.push(pt);
                    }
                }

                let mut mesh = egui::Mesh::default();
                mesh.vertices.push(egui::epaint::Vertex {
                    pos: center,
                    uv: egui::epaint::WHITE_UV,
                    color: grad_color(Vec2::ZERO),
                });
                for &off in &ring {
                    mesh.vertices.push(egui::epaint::Vertex {
                        pos: center + off,
                        uv: egui::epaint::WHITE_UV,
                        color: grad_color(off),
                    });
                }
                let n = ring.len() as u32;
                for i in 0..n {
                    mesh.indices.push(0);
                    mesh.indices.push(1 + i);
                    mesh.indices.push(1 + ((i + 1) % n));
                }
                painter.add(egui::Shape::mesh(mesh));
            }
            _ => {
                // Clean 8-tooth settings cog
                painter.circle_stroke(center, 4.2, Stroke::new(2.0_f32, color));
                for k in 0..8 {
                    let ang = k as f32 * std::f32::consts::TAU / 8.0;
                    painter.line_segment(
                        [
                            center + Vec2::new(ang.cos() * 5.2, ang.sin() * 5.2),
                            center + Vec2::new(ang.cos() * 7.8, ang.sin() * 7.8),
                        ],
                        Stroke::new(2.1_f32, color),
                    );
                }
            }
        }
    }

    fn draw_top_dock(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let accent = self.config.accent();
        let dock_fill = self.config.dock_fill();

        // Top-left: Bold UK Clock & Date anchored cleanly in the top-left corner
        if self.config.show_header {
            let time_str = self.cached_time_str.clone();
            let date_str = self.cached_date_str.clone();
            egui::Area::new(egui::Id::new("classic_top_clock"))
                .anchor(Align2::LEFT_TOP, Vec2::new(28.0, 14.0))
                .order(egui::Order::Middle)
                .show(ctx, |ui| {
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 1.0;
                        ui.label(
                            RichText::new(&time_str)
                                .font(FontId::proportional(40.0))
                                .strong()
                                .color(Color32::WHITE),
                        );
                        ui.label(
                            RichText::new(&date_str)
                                .font(FontId::proportional(13.5))
                                .color(Color32::from_rgba_unmultiplied(230, 238, 250, 215)),
                        );
                    });
                });
        }

        // Reserve top space below the top-right 7-icon capsule
        ui.add_space(44.0);

        egui::Area::new(egui::Id::new("top_dock_capsule"))
            .anchor(Align2::RIGHT_TOP, Vec2::new(-28.0, 14.0))
            .order(egui::Order::Middle)
            .show(ctx, |ui| {
                egui::Frame::NONE
                    .fill(dock_fill)
                    .stroke(Stroke::new(
                        1.0_f32,
                        Color32::from_rgba_unmultiplied(255, 255, 255, 42),
                    ))
                    .corner_radius(CornerRadius::same(22))
                    .inner_margin(egui::Margin::symmetric(10, 6))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;
                            let items = self.dock_items();
                            for (pos, (action_id, tip)) in items.iter().copied().enumerate() {
                                let focused =
                                    self.modal == ActiveModal::None && self.focus == FocusTarget::Dock(pos);
                                let anim = ctx.animate_bool_with_time(
                                    egui::Id::new(("dock_focus", action_id)),
                                    focused,
                                    0.12,
                                );
                                let size = Vec2::splat(34.0);
                                let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
                                let p = ui.painter();

                                let r = 15.5 + anim * 1.8;
                                if anim > 0.01 {
                                    p.circle_filled(
                                        rect.center(),
                                        r,
                                        Color32::from_rgba_unmultiplied(
                                            accent.r(),
                                            accent.g(),
                                            accent.b(),
                                            (anim * 70.0) as u8,
                                        ),
                                    );
                                    p.circle_stroke(
                                        rect.center(),
                                        r,
                                        Stroke::new(1.0_f32 + anim, accent),
                                    );
                                } else {
                                    p.circle_filled(
                                        rect.center(),
                                        15.5,
                                        Color32::from_rgba_unmultiplied(255, 255, 255, 18),
                                    );
                                }

                                let icon_color = if action_id == 1 {
                                    if self.status.is_vpn_active {
                                        if focused {
                                            Color32::from_rgb(74, 222, 128)
                                        } else {
                                            Color32::from_rgb(52, 211, 153)
                                        }
                                    } else if focused {
                                        Color32::from_rgb(252, 138, 138)
                                    } else {
                                        Color32::from_rgb(248, 113, 113)
                                    }
                                } else if action_id == 7 && !focused {
                                    accent
                                } else if focused {
                                    Color32::WHITE
                                } else {
                                    Color32::from_rgb(225, 232, 245)
                                };
                                Self::draw_dock_icon(p, rect.center(), action_id, icon_color);

                                // Live status indicator dots for Wi-Fi (0), VPN (1), and Settings updates (6)
                                if action_id == 0 {
                                    let dot_c = if self.status.is_connected {
                                        Color32::from_rgb(52, 211, 153)
                                    } else {
                                        Color32::from_rgb(248, 113, 113)
                                    };
                                    p.circle_filled(
                                        rect.right_bottom() + Vec2::new(-5.0, -5.0),
                                        3.2,
                                        dot_c,
                                    );
                                } else if action_id == 1 {
                                    let dot_c = if self.status.is_vpn_active {
                                        Color32::from_rgb(52, 211, 153)
                                    } else {
                                        Color32::from_rgb(248, 113, 113)
                                    };
                                    p.circle_filled(
                                        rect.right_bottom() + Vec2::new(-5.0, -5.0),
                                        3.2,
                                        dot_c,
                                    );
                                } else if action_id == 6 && self.has_update_available() {
                                    p.circle_filled(
                                        rect.right_top() + Vec2::new(-4.5, 4.5),
                                        3.8,
                                        Color32::from_rgb(52, 211, 153),
                                    );
                                }

                                if resp.clicked() {
                                    self.focus = FocusTarget::Dock(pos);
                                    self.trigger_dock_action(action_id, ctx);
                                }
                                resp.on_hover_text(tip);
                            }
                        });
                    });
            });
    }

    fn draw_hero_header(&mut self, ui: &mut Ui) {
        if !self.config.show_header {
            return;
        }
        let accent = self.config.accent();

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            // Right: Interactive Weather Block (flush-right below top-right dock, while clock sits top-left)
            if self.config.show_weather {
                let weather_w = 294.0;
                let gap = (ui.available_width() - weather_w).max(16.0);
                ui.add_space(gap);

                let focused =
                    self.modal == ActiveModal::None && self.focus == FocusTarget::Weather;
                let fill = if focused {
                    Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 46)
                } else {
                    Color32::TRANSPARENT
                };
                let stroke = if focused {
                    Stroke::new(2.0_f32, accent)
                } else {
                    Stroke::NONE
                };

                let frame = egui::Frame::NONE
                    .fill(fill)
                    .stroke(stroke)
                    .corner_radius(CornerRadius::same(16))
                    .inner_margin(egui::Margin::symmetric(12, 8));

                let resp = frame
                    .show(ui, |ui| {
                        ui.set_min_width(weather_w - 24.0);
                        ui.horizontal(|ui| {
                            if let Some(w) = &self.weather {
                                let (wr, _) = ui.allocate_exact_size(Vec2::splat(30.0), Sense::hover());
                                Self::draw_weather_icon(ui.painter(), wr.center(), w.condition, 1.15);
                                ui.label(
                                    RichText::new(self.fmt_temp(w.temp_c))
                                        .font(FontId::proportional(38.0))
                                        .strong()
                                        .color(Color32::WHITE),
                                );
                                ui.add_space(10.0);
                                ui.vertical(|ui| {
                                    ui.label(
                                        RichText::new(w.condition)
                                            .font(FontId::proportional(14.5))
                                            .strong()
                                            .color(Color32::WHITE),
                                    );
                                    ui.label(
                                        RichText::new(format!(
                                            "H:{}  L:{}  •  Rain {}%",
                                            self.fmt_temp(w.high_c),
                                            self.fmt_temp(w.low_c),
                                            w.precip_prob
                                        ))
                                        .font(FontId::proportional(12.0))
                                        .color(Color32::from_rgba_unmultiplied(
                                            222, 232, 248, 225,
                                        )),
                                    );
                                    ui.label(
                                        RichText::new("5-Day Forecast ›")
                                            .font(FontId::proportional(12.0))
                                            .strong()
                                            .color(accent),
                                    );
                                });
                            } else {
                                ui.vertical(|ui| {
                                    ui.label(
                                        RichText::new("London Weather")
                                            .font(FontId::proportional(16.0))
                                            .strong()
                                            .color(Color32::WHITE),
                                    );
                                    ui.label(
                                        RichText::new("Loading forecast...")
                                            .font(FontId::proportional(12.0))
                                            .color(Color32::from_rgb(195, 210, 230)),
                                    );
                                });
                            }
                        });
                    })
                    .response
                    .interact(Sense::click());

                if resp.clicked() {
                    self.focus = FocusTarget::Weather;
                    self.modal = ActiveModal::Weather;
                    self.modal_focus = 0;
                }
            }
        });
        ui.add_space(10.0);
    }

    fn draw_app_tile(
        &mut self,
        ui: &mut Ui,
        ctx: &egui::Context,
        app: &TvAppInfo,
        target: FocusTarget,
        tile_w: f32,
        icon_d: f32,
    ) -> (bool, bool) {
        self.request_icon(&app.package_name, ctx);
        let focused = self.modal == ActiveModal::None && self.focus == target;
        let accent = self.config.accent();
        let tile_h = if self.config.show_labels {
            icon_d + 36.0
        } else {
            icon_d + 16.0
        };

        let (rect, resp) = ui.allocate_exact_size(Vec2::new(tile_w, tile_h), Sense::click());
        if focused && self.focus_changed {
            ui.scroll_to_rect(rect.expand(24.0), Some(Align::Center));
            self.focus_changed = false;
        }

        let p = ui.painter();
        let focus_t = ctx.animate_bool_with_time(
            egui::Id::new(("tile_focus", &app.package_name, matches!(target, FocusTarget::Fav(_)))),
            focused,
            0.065,
        );
        let (float_y, breath) = self.resting_focus_motion(focused, focus_t);
        let base_center = Pos2::new(rect.center().x, rect.top() + icon_d * 0.5 + 8.0 - focus_t * 2.5);
        let center = Pos2::new(base_center.x, base_center.y - float_y);
        let radius = icon_d * (0.48 + focus_t * 0.055 + breath * 0.008);

        // Grounded drop shadow (softens & expands slightly as icon levitates)
        p.circle_filled(
            base_center + Vec2::new(0.0, 4.5 + focus_t * 2.5),
            radius + 1.0 + breath * 2.0,
            Color32::from_rgba_unmultiplied(0, 0, 0, (90.0 + focus_t * 50.0 - breath * 14.0) as u8),
        );

        // Smooth animated focus glow halo + breathing resting aura
        if focus_t > 0.01 {
            if breath > 0.01 {
                p.circle_filled(
                    center,
                    radius + 9.5 + breath * 4.0,
                    Color32::from_rgba_unmultiplied(
                        accent.r(),
                        accent.g(),
                        accent.b(),
                        (breath * 34.0) as u8,
                    ),
                );
            }
            p.circle_filled(
                center,
                radius + 6.5 * focus_t + breath * 1.8,
                Color32::from_rgba_unmultiplied(
                    accent.r(),
                    accent.g(),
                    accent.b(),
                    (focus_t * 68.0 + breath * 24.0) as u8,
                ),
            );
        }
        self.draw_focus_effects(ctx, p, center, radius, focused, accent);

        // Circular icon texture or fallback initial
        if let Some(tex) = self.icon_textures.get(&app.package_name) {
            let icon_rect = Rect::from_center_size(center, Vec2::splat(radius * 2.0));
            p.image(
                tex.id(),
                icon_rect,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        } else {
            p.circle_filled(center, radius, Color32::from_rgb(36, 46, 68));
            let initial = app.label.chars().next().unwrap_or('A').to_uppercase().to_string();
            p.text(
                center,
                Align2::CENTER_CENTER,
                initial,
                FontId::proportional(radius * 0.72),
                Color32::WHITE,
            );
        }

        // Crisp circular glass rim + orbital resting specular shimmer
        p.circle_stroke(
            center,
            radius,
            if focused {
                Stroke::new(2.6_f32, accent)
            } else {
                Stroke::new(1.2_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 55))
            },
        );
        self.draw_resting_focus_overlay(ctx, p, center, radius, focused, focus_t, accent);

        // Label
        if self.config.show_labels {
            let mut label = self.display_label(app);
            if label.chars().count() > 14 {
                label = format!("{}…", label.chars().take(13).collect::<String>());
            }
            p.text(
                Pos2::new(rect.center().x, center.y + radius + 14.0),
                Align2::CENTER_CENTER,
                label,
                FontId::proportional(if focused { 13.0 } else { 12.0 }),
                if focused {
                    Color32::WHITE
                } else {
                    Color32::from_rgba_unmultiplied(230, 236, 246, 220)
                },
            );
        }

        let clicked = resp.clicked();
        let right_clicked = resp.secondary_clicked();
        (clicked, right_clicked)
    }

    fn draw_a2ui_surfaces(&self, ui: &mut Ui) {
        if self.a2ui.surfaces.is_empty() {
            return;
        }
        let accent = self.config.accent();
        let card_fill = self.config.dock_fill();

        for surface in self.a2ui.surfaces.values() {
            let root = surface.components.get("root");
            let title = root
                .and_then(|c| c.props.get("title"))
                .and_then(|v| v.as_str())
                .unwrap_or(&surface.surface_id);

            ui.add_space(6.0);
            ui.label(
                RichText::new(title)
                    .font(FontId::proportional(14.5))
                    .strong()
                    .color(accent),
            );
            ui.add_space(4.0);

            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(12.0, 10.0);
                for comp in surface.components.values() {
                    if comp.id == "root" {
                        continue;
                    }
                    let c_title = comp
                        .props
                        .get("title")
                        .and_then(|v| v.as_str())
                        .unwrap_or(&comp.component);
                    let c_val = comp
                        .props
                        .get("value")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let c_sub = comp
                        .props
                        .get("subtitle")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");

                    egui::Frame::NONE
                        .fill(card_fill)
                        .stroke(Stroke::new(
                            1.0,
                            Color32::from_rgba_unmultiplied(255, 255, 255, 36),
                        ))
                        .corner_radius(CornerRadius::same(14))
                        .inner_margin(egui::Margin::symmetric(14, 10))
                        .show(ui, |ui| {
                            ui.set_min_width(180.0);
                            ui.vertical(|ui| {
                                ui.label(
                                    RichText::new(c_title)
                                        .font(FontId::proportional(12.0))
                                        .color(Color32::from_rgb(185, 200, 224)),
                                );
                                if !c_val.is_empty() {
                                    ui.label(
                                        RichText::new(c_val)
                                            .font(FontId::proportional(18.0))
                                            .strong()
                                            .color(Color32::WHITE),
                                    );
                                }
                                if !c_sub.is_empty() {
                                    ui.label(
                                        RichText::new(c_sub)
                                            .font(FontId::proportional(11.0))
                                            .color(accent),
                                    );
                                }
                            });
                        });
                }
            });
            ui.add_space(8.0);
        }
    }

    fn draw_toggle_pill(
        ctx: &egui::Context,
        p: &egui::Painter,
        rect: Rect,
        is_on: bool,
        focused: bool,
        accent: Color32,
        salt: usize,
    ) {
        let anim_t = ctx.animate_bool_with_time(
            egui::Id::new(("settings_toggle_pill", salt)),
            is_on,
            0.12,
        );
        let r = rect.height() * 0.5;
        let track_fill = if is_on {
            Color32::from_rgba_unmultiplied(
                accent.r(),
                accent.g(),
                accent.b(),
                if focused { 235 } else { 185 },
            )
        } else {
            Color32::from_rgba_unmultiplied(255, 255, 255, if focused { 52 } else { 32 })
        };
        p.rect_filled(rect, CornerRadius::same(r as u8), track_fill);
        p.rect_stroke(
            rect,
            CornerRadius::same(r as u8),
            if focused {
                Stroke::new(1.3_f32, Color32::WHITE)
            } else if is_on {
                Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 75))
            } else {
                Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 42))
            },
            egui::StrokeKind::Inside,
        );
        let pad = 3.0_f32;
        let thumb_r = r - pad;
        let x0 = rect.left() + r;
        let x1 = rect.right() - r;
        let cx = x0 + (x1 - x0) * anim_t;
        let cy = rect.center().y;
        p.circle_filled(
            Pos2::new(cx, cy + 0.8),
            thumb_r,
            Color32::from_rgba_unmultiplied(0, 0, 0, 75),
        );
        p.circle_filled(Pos2::new(cx, cy), thumb_r, Color32::WHITE);
    }

    fn draw_settings_live_preview(
        &self,
        ctx: &egui::Context,
        p: &egui::Painter,
        rect: Rect,
        tab: usize,
        row_idx: usize,
        visible_count: usize,
    ) {
        let accent = self.config.accent();
        let accent2 = self.config.accent2();
        let [tr, tg, tb] = self.config.astryx_theme.surface_tint();
        let time_str = if self.cached_time_str.is_empty() {
            "21:42"
        } else {
            self.cached_time_str.as_str()
        };
        let date_short = if self.cached_date_short.is_empty() {
            "Thu 27 Nov"
        } else {
            self.cached_date_short.as_str()
        };
        let city_short = self
            .config
            .weather_city
            .split('·')
            .next()
            .unwrap_or("London")
            .trim();
        let temp_str = self
            .weather
            .as_ref()
            .map(|w| format!("{} · {}", self.fmt_temp(w.temp_c), w.condition))
            .unwrap_or_else(|| "14°C · Clear".to_string());

        // Base canvas fill + soft subtle corner frame
        p.rect_filled(
            rect,
            CornerRadius::same(13),
            Color32::from_rgba_unmultiplied(
                tr.saturating_sub(4),
                tg.saturating_sub(4),
                tb.saturating_sub(2),
                240,
            ),
        );

        // Smooth GPU-interpolated radial gradient glow (triangle fan: zero concentric ring banding at 4K)
        let soft_glow = |center: Pos2, max_r: f32, col: Color32, peak_alpha: u8| {
            let mut mesh = egui::Mesh::default();
            let center_c = Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), peak_alpha);
            let edge_c = Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), 0);
            mesh.colored_vertex(center, center_c);
            let segs = 24usize;
            for i in 0..segs {
                let a = (i as f32 / segs as f32) * std::f32::consts::TAU;
                mesh.colored_vertex(
                    center + Vec2::new(a.cos() * max_r, a.sin() * max_r),
                    edge_c,
                );
            }
            for i in 0..segs {
                let next = (i + 1) % segs;
                mesh.add_triangle(0, (1 + i) as u32, (1 + next) as u32);
            }
            p.add(egui::Shape::mesh(mesh));
        };

        // ---------------------------------------------------------------------
        // MODE A: Live Screensaver Style Preview (Appearance rows 2 & 3)
        // ---------------------------------------------------------------------
        if tab == 0 && (row_idx == 2 || row_idx == 3) {
            // Silky GPU radial ambient glow
            soft_glow(
                Pos2::new(rect.left() + rect.width() * 0.22, rect.top() + 34.0),
                86.0,
                accent,
                56,
            );
            soft_glow(
                Pos2::new(rect.right() - rect.width() * 0.22, rect.bottom() - 28.0),
                82.0,
                accent2,
                50,
            );

            let c = Pos2::new(rect.center().x, rect.center().y - 8.0);
            match self.config.screensaver_style {
                ScreensaverStyle::Classic => {
                    p.text(
                        c + Vec2::new(0.0, -14.0),
                        Align2::CENTER_CENTER,
                        time_str,
                        FontId::proportional(38.0),
                        Color32::WHITE,
                    );
                    p.text(
                        c + Vec2::new(0.0, 14.0),
                        Align2::CENTER_CENTER,
                        date_short,
                        FontId::proportional(11.5),
                        Color32::from_rgb(206, 220, 242),
                    );
                    p.text(
                        c + Vec2::new(0.0, 30.0),
                        Align2::CENTER_CENTER,
                        &temp_str,
                        FontId::proportional(11.0),
                        accent,
                    );
                }
                ScreensaverStyle::GlassCard => {
                    let card = Rect::from_center_size(c, Vec2::new(205.0, 96.0));
                    p.rect_filled(
                        card,
                        CornerRadius::same(16),
                        Color32::from_rgba_unmultiplied(12, 18, 34, 195),
                    );
                    p.rect_stroke(
                        card,
                        CornerRadius::same(16),
                        Stroke::new(
                            1.3_f32,
                            Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 155),
                        ),
                        egui::StrokeKind::Inside,
                    );
                    p.text(
                        c + Vec2::new(0.0, -18.0),
                        Align2::CENTER_CENTER,
                        time_str,
                        FontId::proportional(32.0),
                        Color32::WHITE,
                    );
                    p.rect_filled(
                        Rect::from_center_size(c + Vec2::new(0.0, 4.0), Vec2::new(56.0, 2.0)),
                        CornerRadius::same(1),
                        accent,
                    );
                    p.text(
                        c + Vec2::new(0.0, 18.0),
                        Align2::CENTER_CENTER,
                        date_short,
                        FontId::proportional(10.5),
                        Color32::from_rgb(206, 220, 242),
                    );
                    p.text(
                        c + Vec2::new(0.0, 32.0),
                        Align2::CENTER_CENTER,
                        format!("{city_short} · {temp_str}"),
                        FontId::proportional(10.0),
                        accent,
                    );
                }
                ScreensaverStyle::MinimalClock => {
                    p.text(
                        c,
                        Align2::CENTER_CENTER,
                        time_str,
                        FontId::proportional(48.0),
                        Color32::from_rgba_unmultiplied(255, 255, 255, 242),
                    );
                    p.rect_filled(
                        Rect::from_center_size(c + Vec2::new(0.0, 30.0), Vec2::new(42.0, 1.5)),
                        CornerRadius::same(1),
                        Color32::from_rgba_unmultiplied(255, 255, 255, 80),
                    );
                }
                ScreensaverStyle::EditorialSplit => {
                    let card = Rect::from_center_size(c, Vec2::new(248.0, 74.0));
                    p.rect_filled(
                        card,
                        CornerRadius::same(14),
                        Color32::from_rgba_unmultiplied(8, 12, 22, 165),
                    );
                    p.rect_stroke(
                        card,
                        CornerRadius::same(14),
                        Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 28)),
                        egui::StrokeKind::Inside,
                    );
                    p.text(
                        Pos2::new(c.x - 16.0, c.y),
                        Align2::RIGHT_CENTER,
                        time_str,
                        FontId::proportional(33.0),
                        Color32::WHITE,
                    );
                    p.rect_filled(
                        Rect::from_center_size(Pos2::new(c.x - 4.0, c.y), Vec2::new(2.5, 42.0)),
                        CornerRadius::same(2),
                        accent,
                    );
                    p.text(
                        Pos2::new(c.x + 8.0, c.y - 13.0),
                        Align2::LEFT_CENTER,
                        date_short.to_uppercase(),
                        FontId::proportional(10.5),
                        Color32::from_rgb(228, 236, 250),
                    );
                    p.text(
                        Pos2::new(c.x + 8.0, c.y + 1.0),
                        Align2::LEFT_CENTER,
                        city_short,
                        FontId::proportional(10.0),
                        Color32::from_rgb(164, 180, 206),
                    );
                    p.text(
                        Pos2::new(c.x + 8.0, c.y + 15.0),
                        Align2::LEFT_CENTER,
                        &temp_str,
                        FontId::proportional(10.0),
                        accent,
                    );
                }
                ScreensaverStyle::NeonRing => {
                    let r = 52.0_f32;
                    p.circle_filled(c, r - 3.0, Color32::from_rgba_unmultiplied(8, 12, 24, 190));
                    p.circle_stroke(
                        c,
                        r + 4.0,
                        Stroke::new(
                            1.1_f32,
                            Color32::from_rgba_unmultiplied(accent2.r(), accent2.g(), accent2.b(), 105),
                        ),
                    );
                    p.circle_stroke(
                        c,
                        r,
                        Stroke::new(
                            2.2_f32,
                            Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 220),
                        ),
                    );
                    p.text(
                        c + Vec2::new(0.0, -10.0),
                        Align2::CENTER_CENTER,
                        time_str,
                        FontId::proportional(24.0),
                        Color32::WHITE,
                    );
                    p.text(
                        c + Vec2::new(0.0, 10.0),
                        Align2::CENTER_CENTER,
                        date_short,
                        FontId::proportional(9.5),
                        Color32::from_rgb(206, 220, 242),
                    );
                    p.text(
                        c + Vec2::new(0.0, 24.0),
                        Align2::CENTER_CENTER,
                        &temp_str,
                        FontId::proportional(9.5),
                        accent,
                    );
                }
            }

            // Bottom 5-style indicator dots + idle timer badge
            let active_idx = match self.config.screensaver_style {
                ScreensaverStyle::Classic => 0,
                ScreensaverStyle::GlassCard => 1,
                ScreensaverStyle::MinimalClock => 2,
                ScreensaverStyle::EditorialSplit => 3,
                ScreensaverStyle::NeonRing => 4,
            };
            for i in 0..5usize {
                let x = rect.center().x + (i as f32 - 2.0) * 14.0;
                let y = rect.bottom() - 12.0;
                p.circle_filled(
                    Pos2::new(x, y),
                    if i == active_idx { 3.5 } else { 2.2 },
                    if i == active_idx {
                        accent
                    } else {
                        Color32::from_rgba_unmultiplied(255, 255, 255, 65)
                    },
                );
            }
            let idle_txt = match self.config.ambient_timeout_secs {
                0 => "Timer: OFF",
                60 => "Idle: 1 min",
                180 => "Idle: 3 min",
                _ => "Idle: 5 min",
            };
            p.text(
                Pos2::new(rect.right() - 10.0, rect.bottom() - 12.0),
                Align2::RIGHT_CENTER,
                idle_txt,
                FontId::proportional(9.5),
                Color32::from_rgb(168, 184, 210),
            );
        }
        // ---------------------------------------------------------------------
        // MODE B: Live Animated Gemini Plasma Orb Preview (Gemini AI Tab)
        // ---------------------------------------------------------------------
        else if tab == 2 {
            let t = self.start_time.elapsed().as_secs_f32();
            if !self.config.lightweight_mode {
                ctx.request_repaint_after(Duration::from_millis(45));
            }
            let orb_c = Pos2::new(rect.center().x, rect.center().y - 12.0);
            let active = self.config.gemini_enabled;
            let base_r = 28.0_f32;

            // Soft feathered backdrop nebula
            soft_glow(
                orb_c,
                74.0,
                if active {
                    Color32::from_rgb(139, 92, 246)
                } else {
                    Color32::from_rgb(100, 116, 139)
                },
                if active { 48 } else { 18 },
            );

            // Expanding sonar rings
            for ri in 0..2 {
                let phase = ((t * 0.45 + ri as f32 * 0.5) % 1.0).clamp(0.0, 1.0);
                let r = base_r + 8.0 + phase * 26.0;
                let a = ((1.0 - phase) * if active { 95.0 } else { 32.0 }) as u8;
                let rc = if ri == 0 { accent } else { accent2 };
                p.circle_stroke(
                    orb_c,
                    r,
                    Stroke::new(1.3_f32, Color32::from_rgba_unmultiplied(rc.r(), rc.g(), rc.b(), a)),
                );
            }

            // 20-bar gentle ambient soundwave equalizer
            for b in 0..20usize {
                let ang = (b as f32 / 20.0) * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
                let dir = Vec2::new(ang.cos(), ang.sin());
                let wave = ((ang * 3.0 + t * 2.6).sin() * 0.5 + 0.5)
                    * ((ang * 2.0 - t * 1.9).cos() * 0.5 + 0.5);
                let len = if active { 3.5 + wave * 7.5 } else { 2.0 };
                let bc = if b % 2 == 0 { accent } else { accent2 };
                p.line_segment(
                    [orb_c + dir * (base_r + 5.0), orb_c + dir * (base_r + 5.0 + len)],
                    Stroke::new(
                        2.0_f32,
                        Color32::from_rgba_unmultiplied(bc.r(), bc.g(), bc.b(), if active { 155 } else { 45 }),
                    ),
                );
            }

            // Translucent sphere + color plasma lobes + GPU-gradient Gemini star
            p.circle_filled(orb_c, base_r, Color32::from_rgba_unmultiplied(12, 16, 34, 220));
            if active {
                let drift = Vec2::new((t * 1.4).cos() * 7.0, (t * 1.1).sin() * 6.0);
                p.circle_filled(
                    orb_c + drift,
                    18.0,
                    Color32::from_rgba_unmultiplied(56, 189, 248, 72),
                );
                p.circle_filled(
                    orb_c - drift,
                    18.0,
                    Color32::from_rgba_unmultiplied(168, 85, 247, 72),
                );
            }
            p.circle_stroke(
                orb_c,
                base_r,
                Stroke::new(
                    1.6_f32,
                    if active {
                        Color32::from_rgba_unmultiplied(192, 132, 252, 210)
                    } else {
                        Color32::from_rgba_unmultiplied(255, 255, 255, 55)
                    },
                ),
            );
            Self::draw_dock_icon(p, orb_c, 7, Color32::WHITE);

            // Bottom model + API key pills
            let has_key = !self.config.gemini_api_key.trim().is_empty();
            let model_pill = Rect::from_min_size(
                Pos2::new(rect.left() + 10.0, rect.bottom() - 26.0),
                Vec2::new(150.0, 18.0),
            );
            p.rect_filled(
                model_pill,
                CornerRadius::same(6),
                Color32::from_rgba_unmultiplied(255, 255, 255, 18),
            );
            p.text(
                model_pill.center(),
                Align2::CENTER_CENTER,
                &self.config.gemini_model,
                FontId::proportional(9.5),
                Color32::from_rgb(224, 232, 248),
            );

            let key_pill = Rect::from_min_size(
                Pos2::new(rect.right() - 122.0, rect.bottom() - 26.0),
                Vec2::new(112.0, 18.0),
            );
            let key_c = if has_key {
                Color32::from_rgb(52, 211, 153)
            } else {
                Color32::from_rgb(251, 191, 36)
            };
            p.rect_filled(
                key_pill,
                CornerRadius::same(6),
                Color32::from_rgba_unmultiplied(key_c.r(), key_c.g(), key_c.b(), 34),
            );
            p.text(
                key_pill.center(),
                Align2::CENTER_CENTER,
                if has_key { "API Key Ready" } else { "Set Key :8787" },
                FontId::proportional(9.5),
                key_c,
            );
        }
        // ---------------------------------------------------------------------
        // MODE C: Live Miniature TV Home Screen Preview (Appearance + Weather/HUD)
        // ---------------------------------------------------------------------
        else if tab == 0 || tab == 1 {
            // Soft theme glow blobs (hidden in Lightweight Mode so toggle visibly responds!)
            if !self.config.lightweight_mode {
                soft_glow(
                    Pos2::new(rect.left() + 56.0, rect.top() + 34.0),
                    78.0,
                    accent,
                    42,
                );
                soft_glow(
                    Pos2::new(rect.right() - 52.0, rect.bottom() - 34.0),
                    72.0,
                    accent2,
                    36,
                );
            }

            // Top-left Clock & Weather Header
            if self.config.show_header {
                p.text(
                    Pos2::new(rect.left() + 14.0, rect.top() + 20.0),
                    Align2::LEFT_CENTER,
                    time_str,
                    FontId::proportional(21.0),
                    Color32::WHITE,
                );
                let sub_line = if self.config.show_weather {
                    format!("{date_short} · {city_short} {temp_str}")
                } else {
                    date_short.to_string()
                };
                p.text(
                    Pos2::new(rect.left() + 14.0, rect.top() + 37.0),
                    Align2::LEFT_CENTER,
                    sub_line,
                    FontId::proportional(8.8),
                    Color32::from_rgb(200, 214, 236),
                );
            } else {
                p.text(
                    Pos2::new(rect.left() + 14.0, rect.top() + 18.0),
                    Align2::LEFT_CENTER,
                    "Header Hidden",
                    FontId::proportional(9.0),
                    Color32::from_rgba_unmultiplied(255, 255, 255, 72),
                );
            }

            // Live Status Pill
            if self.config.show_status_pill {
                let sp = Rect::from_min_size(
                    Pos2::new(rect.left() + 14.0, rect.top() + 47.0),
                    Vec2::new(112.0, 14.0),
                );
                p.rect_filled(sp, CornerRadius::same(7), Color32::from_rgba_unmultiplied(0, 0, 0, 96));
                p.rect_stroke(
                    sp,
                    CornerRadius::same(7),
                    Stroke::new(0.8_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 36)),
                    egui::StrokeKind::Inside,
                );
                p.circle_filled(
                    Pos2::new(sp.left() + 7.0, sp.center().y),
                    2.6,
                    if self.status.is_connected {
                        Color32::from_rgb(52, 211, 153)
                    } else {
                        Color32::from_rgb(248, 113, 113)
                    },
                );
                p.text(
                    Pos2::new(sp.left() + 14.0, sp.center().y),
                    Align2::LEFT_CENTER,
                    format!(
                        "Wi-Fi · VPN {}",
                        if self.status.is_vpn_active { "ON" } else { "OFF" }
                    ),
                    FontId::proportional(8.0),
                    Color32::from_rgb(218, 228, 246),
                );
            }

            // Optional Performance HUD badge
            if self.config.show_fps_meter {
                let hud = Rect::from_center_size(
                    Pos2::new(rect.center().x + 8.0, rect.top() + 16.0),
                    Vec2::new(54.0, 14.0),
                );
                p.rect_filled(hud, CornerRadius::same(5), Color32::from_rgba_unmultiplied(6, 12, 20, 185));
                p.rect_stroke(
                    hud,
                    CornerRadius::same(5),
                    Stroke::new(0.8_f32, Color32::from_rgb(52, 211, 153)),
                    egui::StrokeKind::Inside,
                );
                p.text(
                    hud.center(),
                    Align2::CENTER_CENTER,
                    format!("{:.0} FPS", self.display_hz.max(60.0)),
                    FontId::proportional(8.0),
                    Color32::from_rgb(52, 211, 153),
                );
            }

            // Top-right mini dock pill (with live Gemini color star!)
            let dock_w = if self.config.gemini_enabled { 72.0 } else { 56.0 };
            let dock_r = Rect::from_min_size(
                Pos2::new(rect.right() - dock_w - 12.0, rect.top() + 12.0),
                Vec2::new(dock_w, 18.0),
            );
            p.rect_filled(
                dock_r,
                CornerRadius::same(9),
                Color32::from_rgba_unmultiplied(0, 0, 0, 108),
            );
            p.rect_stroke(
                dock_r,
                CornerRadius::same(9),
                Stroke::new(0.8_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 34)),
                egui::StrokeKind::Inside,
            );
            let mut dx = dock_r.left() + 11.0;
            p.circle_filled(Pos2::new(dx, dock_r.center().y), 3.0, Color32::from_rgb(52, 211, 153));
            dx += 13.0;
            p.circle_filled(Pos2::new(dx, dock_r.center().y), 3.0, Color32::from_rgb(186, 202, 228));
            if self.config.gemini_enabled {
                dx += 14.0;
                // Mini 4-point Gemini star in preview bar
                let sc = Pos2::new(dx, dock_r.center().y);
                let mut pts = Vec::with_capacity(8);
                for k in 0..8 {
                    let rad = if k % 2 == 0 { 5.2 } else { 1.8 };
                    let a = -std::f32::consts::FRAC_PI_2 + k as f32 * std::f32::consts::FRAC_PI_4;
                    pts.push(sc + Vec2::new(a.cos() * rad, a.sin() * rad));
                }
                p.add(egui::Shape::convex_polygon(
                    pts,
                    Color32::from_rgb(142, 132, 245),
                    Stroke::NONE,
                ));
            }
            p.circle_filled(Pos2::new(dock_r.right() - 11.0, dock_r.center().y), 4.0, accent);

            let tile_colors = [
                Color32::from_rgb(225, 29, 72),
                Color32::from_rgb(124, 58, 237),
                Color32::from_rgb(234, 88, 12),
                Color32::from_rgb(16, 185, 129),
                Color32::from_rgb(14, 165, 233),
                Color32::from_rgb(217, 70, 239),
                Color32::from_rgb(245, 158, 11),
                Color32::from_rgb(59, 130, 246),
            ];

            // Home layout preview: Zenith Horizon Dock vs Classic Grid
            match self.config.layout_style {
                LayoutStyle::Zenith => {
                    if self.config.show_now_playing {
                        let np = Rect::from_min_size(
                            Pos2::new(rect.left() + 14.0, rect.bottom() - 68.0),
                            Vec2::new(138.0, 15.0),
                        );
                        p.rect_filled(np, CornerRadius::same(6), Color32::from_rgba_unmultiplied(0, 0, 0, 112));
                        p.circle_filled(
                            Pos2::new(np.left() + 8.0, np.center().y),
                            2.8,
                            Color32::from_rgb(52, 211, 153),
                        );
                        p.text(
                            Pos2::new(np.left() + 16.0, np.center().y),
                            Align2::LEFT_CENTER,
                            "Now Playing · Media",
                            FontId::proportional(8.0),
                            Color32::from_rgb(220, 230, 246),
                        );
                    }
                    let (tw, th, gap) = match self.config.icon_size {
                        IconSize::Compact => (36.0_f32, 22.0_f32, 8.0_f32),
                        IconSize::Standard => (44.0_f32, 27.0_f32, 9.0_f32),
                        IconSize::Large => (52.0_f32, 32.0_f32, 9.0_f32),
                    };
                    let count = 5usize;
                    let total_w = count as f32 * tw + (count - 1) as f32 * gap;
                    let start_x = rect.center().x - total_w * 0.5;
                    let base_y = rect.bottom() - if self.config.show_labels { 22.0 } else { 14.0 } - th;
                    for i in 0..count {
                        let tr_rect = Rect::from_min_size(
                            Pos2::new(start_x + i as f32 * (tw + gap), base_y),
                            Vec2::new(tw, th),
                        );
                        p.rect_filled(tr_rect, CornerRadius::same(7), tile_colors[i]);
                        if i == 0 {
                            p.rect_stroke(
                                tr_rect.expand(1.5),
                                CornerRadius::same(8),
                                Stroke::new(1.8_f32, accent),
                                egui::StrokeKind::Outside,
                            );
                        }
                        if self.config.show_labels {
                            p.rect_filled(
                                Rect::from_center_size(
                                    Pos2::new(tr_rect.center().x, tr_rect.bottom() + 6.0),
                                    Vec2::new(tw * 0.65, 3.0),
                                ),
                                CornerRadius::same(1),
                                Color32::from_rgba_unmultiplied(255, 255, 255, 165),
                            );
                        }
                    }
                }
                LayoutStyle::Classic => {
                    let (tw, th, gx, gy) = match self.config.icon_size {
                        IconSize::Compact => (46.0_f32, 22.0_f32, 9.0_f32, 13.0_f32),
                        IconSize::Standard => (56.0_f32, 26.0_f32, 10.0_f32, 14.0_f32),
                        IconSize::Large => (64.0_f32, 30.0_f32, 10.0_f32, 14.0_f32),
                    };
                    let cols = 4usize;
                    let total_w = cols as f32 * tw + (cols - 1) as f32 * gx;
                    let start_x = rect.center().x - total_w * 0.5;
                    let start_y = rect.top() + 72.0;
                    for r_i in 0..2usize {
                        for c_i in 0..cols {
                            let idx = r_i * cols + c_i;
                            let tr_rect = Rect::from_min_size(
                                Pos2::new(start_x + c_i as f32 * (tw + gx), start_y + r_i as f32 * (th + gy)),
                                Vec2::new(tw, th),
                            );
                            p.rect_filled(tr_rect, CornerRadius::same(6), tile_colors[idx % tile_colors.len()]);
                            if idx == 0 {
                                p.rect_stroke(
                                    tr_rect.expand(1.2),
                                    CornerRadius::same(7),
                                    Stroke::new(1.8_f32, accent),
                                    egui::StrokeKind::Outside,
                                );
                            }
                            if self.config.show_labels {
                                p.rect_filled(
                                    Rect::from_center_size(
                                        Pos2::new(tr_rect.center().x, tr_rect.bottom() + 4.5),
                                        Vec2::new(tw * 0.58, 2.6),
                                    ),
                                    CornerRadius::same(1),
                                    Color32::from_rgba_unmultiplied(255, 255, 255, 155),
                                );
                            }
                        }
                    }
                }
            }

            // Night dimming veil preview when Auto Night Dimming is focused & ON
            if self.config.auto_night_dim && tab == 0 && row_idx == 7 {
                p.rect_filled(rect, CornerRadius::same(13), Color32::from_rgba_unmultiplied(4, 6, 14, 72));
                p.text(
                    Pos2::new(rect.right() - 10.0, rect.bottom() - 10.0),
                    Align2::RIGHT_CENTER,
                    "Night Dimming Active",
                    FontId::proportional(9.0),
                    Color32::from_rgb(192, 132, 252),
                );
            } else {
                p.text(
                    Pos2::new(rect.right() - 10.0, rect.bottom() - 8.0),
                    Align2::RIGHT_CENTER,
                    format!(
                        "{} · {}",
                        self.config.astryx_theme.label().replace("Astryx ", ""),
                        self.config.weather_scene.label()
                    ),
                    FontId::proportional(8.5),
                    Color32::from_rgba_unmultiplied(210, 224, 246, 175),
                );
            }
        }
        // ---------------------------------------------------------------------
        // MODE D: Live TV Telemetry & Health Card (Apps & System Tabs)
        // ---------------------------------------------------------------------
        else {
            let pad = 14.0_f32;
            // Device header
            p.text(
                Pos2::new(rect.left() + pad, rect.top() + 20.0),
                Align2::LEFT_CENTER,
                format!(
                    "{} · Android {} · {:.0}Hz",
                    self.status.device_model, self.status.android_version, self.display_hz
                ),
                FontId::proportional(11.5),
                Color32::WHITE,
            );
            let vpn_c = if self.status.is_vpn_active {
                Color32::from_rgb(52, 211, 153)
            } else {
                Color32::from_rgb(148, 163, 184)
            };
            let vpn_pill = Rect::from_min_size(
                Pos2::new(rect.right() - pad - 56.0, rect.top() + 11.0),
                Vec2::new(56.0, 17.0),
            );
            p.rect_filled(
                vpn_pill,
                CornerRadius::same(6),
                Color32::from_rgba_unmultiplied(vpn_c.r(), vpn_c.g(), vpn_c.b(), 34),
            );
            p.text(
                vpn_pill.center(),
                Align2::CENTER_CENTER,
                if self.status.is_vpn_active { "VPN ON" } else { "VPN OFF" },
                FontId::proportional(8.8),
                vpn_c,
            );

            // RAM progress bar
            let ram_y = rect.top() + 48.0;
            let ram_used = (self.status.ram_total_mb - self.status.ram_free_mb).max(0) as f32
                / (self.status.ram_total_mb.max(1) as f32);
            p.text(
                Pos2::new(rect.left() + pad, ram_y),
                Align2::LEFT_CENTER,
                "MEMORY (RAM)",
                FontId::proportional(9.5),
                Color32::from_rgb(156, 174, 202),
            );
            p.text(
                Pos2::new(rect.right() - pad, ram_y),
                Align2::RIGHT_CENTER,
                format!("{} / {} MB free", self.status.ram_free_mb, self.status.ram_total_mb),
                FontId::proportional(10.5),
                Color32::WHITE,
            );
            let bar_w = rect.width() - pad * 2.0;
            let ram_bar = Rect::from_min_size(Pos2::new(rect.left() + pad, ram_y + 10.0), Vec2::new(bar_w, 6.0));
            p.rect_filled(ram_bar, CornerRadius::same(3), Color32::from_rgba_unmultiplied(255, 255, 255, 24));
            let mut ram_fill = ram_bar;
            ram_fill.set_width((bar_w * ram_used.clamp(0.06, 1.0)).max(6.0));
            p.rect_filled(ram_fill, CornerRadius::same(3), accent);

            // Storage progress bar
            let sto_y = rect.top() + 86.0;
            let sto_used = ((self.status.storage_total_gb - self.status.storage_free_gb).max(0.0)
                / self.status.storage_total_gb.max(0.1)) as f32;
            p.text(
                Pos2::new(rect.left() + pad, sto_y),
                Align2::LEFT_CENTER,
                "STORAGE",
                FontId::proportional(9.5),
                Color32::from_rgb(156, 174, 202),
            );
            p.text(
                Pos2::new(rect.right() - pad, sto_y),
                Align2::RIGHT_CENTER,
                format!(
                    "{:.1} / {:.1} GB free",
                    self.status.storage_free_gb, self.status.storage_total_gb
                ),
                FontId::proportional(10.5),
                Color32::WHITE,
            );
            let sto_bar = Rect::from_min_size(Pos2::new(rect.left() + pad, sto_y + 10.0), Vec2::new(bar_w, 6.0));
            p.rect_filled(sto_bar, CornerRadius::same(3), Color32::from_rgba_unmultiplied(255, 255, 255, 24));
            let mut sto_fill = sto_bar;
            sto_fill.set_width((bar_w * sto_used.clamp(0.06, 1.0)).max(6.0));
            p.rect_filled(sto_fill, CornerRadius::same(3), Color32::from_rgb(52, 211, 153));

            // Bottom 3 stat pills
            let pill_y = rect.bottom() - 40.0;
            let pill_gap = 8.0_f32;
            let pill_w = (bar_w - pill_gap * 2.0) / 3.0;
            let stats = [
                ("APPS", format!("{} visible", visible_count)),
                ("COMPANION", format!(":{WEB_COMPANION_PORT}")),
                ("VERSION", format!("v{}", self.app_version)),
            ];
            for (i, (k, v)) in stats.iter().enumerate() {
                let pr = Rect::from_min_size(
                    Pos2::new(rect.left() + pad + i as f32 * (pill_w + pill_gap), pill_y),
                    Vec2::new(pill_w, 30.0),
                );
                p.rect_filled(pr, CornerRadius::same(8), Color32::from_rgba_unmultiplied(255, 255, 255, 13));
                p.text(
                    Pos2::new(pr.center().x, pr.top() + 9.0),
                    Align2::CENTER_CENTER,
                    *k,
                    FontId::proportional(8.0),
                    Color32::from_rgb(152, 170, 198),
                );
                p.text(
                    Pos2::new(pr.center().x, pr.bottom() - 9.0),
                    Align2::CENTER_CENTER,
                    v,
                    FontId::proportional(10.5),
                    Color32::WHITE,
                );
            }
        }

        // Outer crisp glass rim
        p.rect_stroke(
            rect,
            CornerRadius::same(13),
            Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 30)),
            egui::StrokeKind::Inside,
        );
    }

    fn draw_modal_shell(
        &mut self,
        ctx: &egui::Context,
        title: &str,
        width: f32,
        height: f32,
        body: impl FnOnce(&mut Self, &mut Ui, &egui::Context),
    ) {
        // Handle Android TV remote BACK button (KEYCODE_BACK) and keyboard Escape/Backspace
        let is_text_modal = matches!(self.modal, ActiveModal::Search | ActiveModal::RenameApp(_));
        let jni_back = bridge::take_back_pressed();
        let key_back = ctx.input(|i| i.key_pressed(Key::Escape))
            || (!is_text_modal && ctx.input(|i| i.key_pressed(Key::Backspace)));
        if (jni_back || key_back) && self.last_back_handled.elapsed() > Duration::from_millis(260) {
            self.last_back_handled = Instant::now();
            self.close_modal_step();
            return;
        }

        let screen = ctx.screen_rect();
        let dim = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Middle,
            egui::Id::new("modal_scrim"),
        ));
        dim.rect_filled(screen, 0.0, Color32::from_rgba_unmultiplied(4, 6, 14, 150));

        let fill = self.config.modal_fill();
        let accent = self.config.accent();
        let pop_scale = if self.config.modal_pop_in && !self.config.lightweight_mode {
            let el = self.modal_opened_at.elapsed().as_secs_f32();
            if el < 0.085 {
                ctx.request_repaint();
                let frac = (el / 0.085).clamp(0.0, 1.0);
                let ease = 1.0 - (1.0 - frac).powi(3);
                0.93 + 0.07 * ease
            } else {
                1.0
            }
        } else {
            1.0
        };
        let w = (width * pop_scale).min(screen.width() - 48.0);
        let h = (height * pop_scale).min(screen.height() - 48.0);

        egui::Area::new(egui::Id::new("active_modal"))
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::NONE
                    .fill(fill)
                    .stroke(Stroke::new(
                        1.5,
                        Color32::from_rgba_unmultiplied(255, 255, 255, 52),
                    ))
                    .corner_radius(CornerRadius::same(22))
                    .inner_margin(egui::Margin::same(20))
                    .show(ui, |ui| {
                        ui.set_width(w);
                        ui.set_max_height(h);

                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(title)
                                    .font(FontId::proportional(20.0))
                                    .strong()
                                    .color(Color32::WHITE),
                            );
                            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                                let close_focused = self.modal_close_focused;
                                let (cr, resp) =
                                    ui.allocate_exact_size(Vec2::splat(28.0), Sense::click());
                                let p = ui.painter();
                                let c = cr.center();
                                p.circle_filled(
                                    c,
                                    13.5,
                                    if close_focused {
                                        accent
                                    } else {
                                        Color32::from_rgba_unmultiplied(255, 255, 255, 22)
                                    },
                                );
                                p.circle_stroke(
                                    c,
                                    13.5,
                                    if close_focused {
                                        Stroke::new(2.0_f32, Color32::WHITE)
                                    } else {
                                        Stroke::new(1.0_f32, accent)
                                    },
                                );
                                let x_s = Stroke::new(2.0_f32, Color32::WHITE);
                                p.line_segment(
                                    [c + Vec2::new(-4.5, -4.5), c + Vec2::new(4.5, 4.5)],
                                    x_s,
                                );
                                p.line_segment(
                                    [c + Vec2::new(4.5, -4.5), c + Vec2::new(-4.5, 4.5)],
                                    x_s,
                                );
                                if resp.clicked() {
                                    self.close_modal_step();
                                }
                            });
                        });
                        ui.add_space(8.0);
                        ui.separator();
                        ui.add_space(10.0);

                        ScrollArea::vertical().show(ui, |ui| {
                            body(self, ui, ctx);
                        });
                    });
            });
    }

    fn modal_btn(
        ui: &mut Ui,
        label: &str,
        selected: bool,
        focused: bool,
        accent: Color32,
    ) -> bool {
        let fill = if focused {
            Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 85)
        } else if selected {
            Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 40)
        } else {
            Color32::from_rgba_unmultiplied(255, 255, 255, 18)
        };
        let stroke = if focused {
            Stroke::new(2.0, accent)
        } else if selected {
            Stroke::new(1.2, accent)
        } else {
            Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 32))
        };
        ui.add(
            egui::Button::new(
                RichText::new(label)
                    .font(FontId::proportional(13.0))
                    .strong()
                    .color(Color32::WHITE),
            )
            .fill(fill)
            .stroke(stroke)
            .corner_radius(CornerRadius::same(10))
            .min_size(Vec2::new(72.0, 32.0)),
        )
        .clicked()
    }

    fn render_modals(&mut self, ctx: &egui::Context, visible: &[TvAppInfo]) {
        // Prevent the Enter/OK press (or held auto-repeat from Hold-OK!) that opened a modal
        // from immediately activating item 0 inside the modal until the user lifts their thumb off OK.
        let select_down = ctx.input(|i| i.key_down(Key::Enter) || i.key_down(Key::Space));
        if self.modal_wait_key_release && !select_down {
            self.modal_wait_key_release = false;
        }
        let mut allow_keys = !self.modal_just_opened && !self.modal_wait_key_release;
        self.modal_just_opened = false;

        // If D-Pad focus is on the top-right "✕ Close" button, handle OK / Down cleanly here
        if self.modal_close_focused && allow_keys {
            let (ok, down) = ctx.input(|i| {
                (
                    i.key_pressed(Key::Enter) || i.key_pressed(Key::Space),
                    i.key_pressed(Key::ArrowDown),
                )
            });
            if ok {
                self.close_modal_step();
                return;
            }
            if down {
                self.modal_close_focused = false;
            }
            allow_keys = false;
        }

        match self.modal.clone() {
            ActiveModal::None => {}
            ActiveModal::Welcome => {
                // Focus 0..4 = 2x2 Quick-Setup Cards, Focus 4 = "Start Astryx Launcher" CTA button
                let mut activate = false;
                if allow_keys {
                    ctx.input(|i| {
                        let up = i.key_pressed(Key::ArrowUp);
                        let down = i.key_pressed(Key::ArrowDown);
                        let left = i.key_pressed(Key::ArrowLeft);
                        let right = i.key_pressed(Key::ArrowRight);
                        let ok = i.key_pressed(Key::Enter) || i.key_pressed(Key::Space);

                        match self.modal_focus {
                            0 => {
                                if right { self.modal_focus = 1; }
                                else if down { self.modal_focus = 2; }
                                else if up { self.modal_close_focused = true; }
                            }
                            1 => {
                                if left { self.modal_focus = 0; }
                                else if down { self.modal_focus = 3; }
                                else if up { self.modal_close_focused = true; }
                            }
                            2 => {
                                if right { self.modal_focus = 3; }
                                else if up { self.modal_focus = 0; }
                                else if down { self.modal_focus = 4; }
                            }
                            3 => {
                                if left { self.modal_focus = 2; }
                                else if up { self.modal_focus = 1; }
                                else if down { self.modal_focus = 4; }
                            }
                            _ => {
                                if up { self.modal_focus = 2; }
                            }
                        }
                        if ok {
                            activate = true;
                        }
                    });
                }

                let apply_welcome_card = |this: &mut Self, idx: usize, ctx: &egui::Context| match idx {
                    0 => {
                        this.config.layout_style = match this.config.layout_style {
                            LayoutStyle::Zenith => LayoutStyle::Classic,
                            LayoutStyle::Classic => LayoutStyle::Zenith,
                        };
                        this.save_config();
                    }
                    1 => {
                        let len = WeatherScene::ALL.len();
                        let cur = WeatherScene::ALL
                            .iter()
                            .position(|s| *s == this.config.weather_scene)
                            .unwrap_or(0);
                        this.config.weather_scene = WeatherScene::ALL[(cur + 1) % len];
                        this.save_config();
                        this.ensure_wallpaper(ctx.clone());
                    }
                    2 => {
                        let len = FocusAnimStyle::ALL.len();
                        let cur = FocusAnimStyle::ALL
                            .iter()
                            .position(|a| *a == this.config.focus_anim)
                            .unwrap_or(0);
                        this.config.focus_anim = FocusAnimStyle::ALL[(cur + 1) % len];
                        this.save_config();
                    }
                    3 => {
                        this.config.accent_idx = (this.config.accent_idx + 1) % ACCENT_PRESETS.len();
                        this.save_config();
                    }
                    _ => {
                        this.config.onboarding_done = true;
                        this.save_config();
                        this.modal = ActiveModal::None;
                        this.show_toast("Welcome to Astryx Rust Launcher!");
                    }
                };

                if activate {
                    apply_welcome_card(self, self.modal_focus, ctx);
                    if self.modal == ActiveModal::None {
                        return;
                    }
                }

                self.draw_modal_shell(
                    ctx,
                    "Welcome to Astryx Rust Launcher",
                    820.0,
                    475.0,
                    |this, ui, ctx| {
                        let accent = this.config.accent();
                        let accent_name =
                            ACCENT_PRESETS[this.config.accent_idx % ACCENT_PRESETS.len()].0;

                        // Hero Emblem & Tagline
                        ui.horizontal(|ui| {
                            let (star_r, _) = ui.allocate_exact_size(Vec2::splat(46.0), Sense::hover());
                            let p = ui.painter();
                            let c = star_r.center();
                            p.circle_filled(
                                c,
                                22.0,
                                Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 45),
                            );
                            p.circle_stroke(c, 22.0, Stroke::new(1.8_f32, accent));
                            // Crisp 4-point Astryx star
                            let mut pts = Vec::new();
                            for k in 0..8 {
                                let r = if k % 2 == 0 { 14.0 } else { 4.2 };
                                let a = -std::f32::consts::FRAC_PI_2 + k as f32 * std::f32::consts::FRAC_PI_4;
                                pts.push(c + Vec2::new(a.cos() * r, a.sin() * r));
                            }
                            p.add(egui::Shape::convex_polygon(pts, Color32::WHITE, Stroke::NONE));

                            ui.add_space(10.0);
                            ui.vertical(|ui| {
                                ui.label(
                                    RichText::new("Pure Native Rust  ·  4K 60fps OpenGL ES  ·  London UK")
                                        .font(FontId::proportional(13.0))
                                        .strong()
                                        .color(accent),
                                );
                                ui.label(
                                    RichText::new("Your launcher is pre-configured with Zenith Horizon, Cinema Ribbon Settings, and Live 1km UK Satellite. Customize below or press OK to start.")
                                        .font(FontId::proportional(12.5))
                                        .color(Color32::from_rgb(215, 226, 244)),
                                );
                            });
                        });

                        ui.add_space(14.0);

                        // 2x2 Interactive Quick-Setup Cards
                        let cards: [(&str, &str, String, usize); 4] = [
                            ("Home Layout", "Switch Zenith dock or Classic grid", this.config.layout_style.label().to_string(), 0),
                            ("Live Wallpaper", "1km UK Satellite, Space Earth, or Live Sky", this.config.weather_scene.label().to_string(), 1),
                            ("60fps Animation", "Kinetic Ripple, Magnetic Wave, Glow, Halo", this.config.focus_anim.label().to_string(), 3),
                            ("Accent Color", "Glow & highlight theme tint", accent_name.to_string(), 2),
                        ];

                        let mut clicked: Option<usize> = None;
                        for row_i in 0..2 {
                            ui.columns(2, |cols| {
                                for c_i in 0..2 {
                                    let idx = row_i * 2 + c_i;
                                    let (title, desc, val, theme_ic) = &cards[idx];
                                    let focused = this.modal_focus == idx;
                                    let w = cols[c_i].available_width();
                                    let (rect, resp) = cols[c_i]
                                        .allocate_exact_size(Vec2::new(w, 72.0), Sense::click());
                                    let p = cols[c_i].painter();

                                    p.rect_filled(
                                        rect,
                                        CornerRadius::same(14),
                                        if focused {
                                            Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 52)
                                        } else {
                                            Color32::from_rgba_unmultiplied(255, 255, 255, 12)
                                        },
                                    );
                                    p.rect_stroke(
                                        rect,
                                        CornerRadius::same(14),
                                        if focused {
                                            Stroke::new(2.0_f32, accent)
                                        } else {
                                            Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 22))
                                        },
                                        egui::StrokeKind::Inside,
                                    );

                                    let ic_c = Pos2::new(rect.left() + 26.0, rect.center().y);
                                    p.circle_filled(
                                        ic_c,
                                        14.0,
                                        if focused {
                                            Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 88)
                                        } else {
                                            Color32::from_rgba_unmultiplied(255, 255, 255, 18)
                                        },
                                    );
                                    Self::draw_theme_icon(p, ic_c, *theme_ic, Color32::WHITE, accent);

                                    p.text(
                                        Pos2::new(rect.left() + 50.0, rect.top() + 24.0),
                                        Align2::LEFT_CENTER,
                                        *title,
                                        FontId::proportional(14.0),
                                        Color32::WHITE,
                                    );
                                    p.text(
                                        Pos2::new(rect.left() + 50.0, rect.bottom() - 22.0),
                                        Align2::LEFT_CENTER,
                                        *desc,
                                        FontId::proportional(11.0),
                                        Color32::from_rgb(185, 200, 224),
                                    );

                                    let pill_w = (val.len() as f32 * 6.8 + 20.0).clamp(64.0, 128.0);
                                    let pill_r = Rect::from_min_size(
                                        Pos2::new(rect.right() - pill_w - 12.0, rect.top() + 14.0),
                                        Vec2::new(pill_w, 24.0),
                                    );
                                    p.rect_filled(
                                        pill_r,
                                        CornerRadius::same(8),
                                        if focused {
                                            Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 125)
                                        } else {
                                            Color32::from_rgba_unmultiplied(52, 211, 153, 42)
                                        },
                                    );
                                    p.text(
                                        pill_r.center(),
                                        Align2::CENTER_CENTER,
                                        val,
                                        FontId::proportional(11.5),
                                        Color32::WHITE,
                                    );

                                    if resp.clicked() {
                                        this.modal_focus = idx;
                                        clicked = Some(idx);
                                    }
                                }
                            });
                            ui.add_space(8.0);
                        }

                        // Live 60fps Focus Animation Preview Stage + Quick Remote Guide
                        ui.add_space(4.0);
                        egui::Frame::NONE
                            .fill(Color32::from_rgba_unmultiplied(10, 14, 24, 150))
                            .stroke(Stroke::new(
                                1.0_f32,
                                Color32::from_rgba_unmultiplied(255, 255, 255, 24),
                            ))
                            .corner_radius(CornerRadius::same(14))
                            .inner_margin(egui::Margin::symmetric(14, 8))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.vertical(|ui| {
                                        ui.label(
                                            RichText::new("LIVE ANIMATION PREVIEW")
                                                .font(FontId::proportional(10.0))
                                                .strong()
                                                .color(accent),
                                        );
                                        ui.label(
                                            RichText::new(this.config.focus_anim.label())
                                                .font(FontId::proportional(14.5))
                                                .strong()
                                                .color(Color32::WHITE),
                                        );
                                        ui.label(
                                            RichText::new("OK Open · Hold OK Options · Down Clear RAM")
                                                .font(FontId::proportional(10.5))
                                                .color(Color32::from_rgb(175, 192, 218)),
                                        );
                                    });

                                    ui.add_space(24.0);

                                    // 3 live animated mini app orbs cycling focus every 1.1s at 60fps
                                    let (stage_rect, _) = ui.allocate_exact_size(
                                        Vec2::new(ui.available_width().max(220.0), 54.0),
                                        Sense::hover(),
                                    );
                                    let p = ui.painter();
                                    let t = this.start_time.elapsed().as_secs_f32();
                                    let cycle_dur = 1.15_f32;
                                    let active_orb = ((t / cycle_dur).floor() as usize) % 3;
                                    let phase = (t % cycle_dur) / cycle_dur; // 0..1 within current orb
                                    let orb_colors = [
                                        Color32::from_rgb(124, 58, 237),
                                        Color32::from_rgb(6, 182, 212),
                                        Color32::from_rgb(59, 130, 246),
                                    ];
                                    let orb_labels = ["S", "T", "N"];
                                    let spacing = 64.0_f32;
                                    let base_x = stage_rect.center().x - spacing;
                                    let base_y = stage_rect.center().y;

                                    // Sliding liquid-glass pill behind active orb
                                    let target_x = base_x + active_orb as f32 * spacing;
                                    let anim_x = ctx.animate_value_with_time(
                                        egui::Id::new("welcome_preview_x"),
                                        target_x,
                                        0.10,
                                    );
                                    p.rect_filled(
                                        Rect::from_center_size(
                                            Pos2::new(anim_x, base_y),
                                            Vec2::new(48.0, 46.0),
                                        ),
                                        CornerRadius::same(14),
                                        Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 38),
                                    );

                                    for k in 0..3 {
                                        let is_act = k == active_orb;
                                        let is_neigh = this.config.focus_anim == FocusAnimStyle::MagneticWave
                                            && active_orb.abs_diff(k) == 1;
                                        let lift = if is_act { -4.0 } else if is_neigh { -2.0 } else { 0.0 };
                                        let r = if is_act { 17.5 } else if is_neigh { 15.5 } else { 13.5 };
                                        let c = Pos2::new(base_x + k as f32 * spacing, base_y + lift);

                                        if is_act {
                                            match this.config.focus_anim {
                                                FocusAnimStyle::KineticRipple => {
                                                    if phase < 0.45 {
                                                        let f = phase / 0.45;
                                                        let ease = 1.0 - (1.0 - f).powi(3);
                                                        let rr = r + 2.0 + ease * 14.0;
                                                        let a = ((1.0 - f) * 220.0) as u8;
                                                        p.circle_stroke(
                                                            c,
                                                            rr,
                                                            Stroke::new(
                                                                2.2_f32 * (1.0 - f * 0.4),
                                                                Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), a),
                                                            ),
                                                        );
                                                    }
                                                }
                                                FocusAnimStyle::AmbientGlow => {
                                                    for (m, al) in [(1.55, 26u8), (1.32, 46u8), (1.14, 68u8)] {
                                                        p.circle_filled(
                                                            c,
                                                            r * m,
                                                            Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), al),
                                                        );
                                                    }
                                                }
                                                FocusAnimStyle::BreathingHalo => {
                                                    let w = (t * 4.2).sin() * 0.5 + 0.5;
                                                    p.circle_stroke(
                                                        c,
                                                        r + 3.0 + w * 4.5,
                                                        Stroke::new(
                                                            2.0_f32,
                                                            Color32::from_rgba_unmultiplied(
                                                                accent.r(),
                                                                accent.g(),
                                                                accent.b(),
                                                                (80.0 + w * 130.0) as u8,
                                                            ),
                                                        ),
                                                    );
                                                }
                                                FocusAnimStyle::MagneticWave | FocusAnimStyle::Minimal => {}
                                            }
                                        }

                                        p.circle_filled(c, r, orb_colors[k]);
                                        p.circle_stroke(
                                            c,
                                            r,
                                            if is_act {
                                                Stroke::new(2.2_f32, Color32::WHITE)
                                            } else {
                                                Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 65))
                                            },
                                        );
                                        p.text(
                                            c,
                                            Align2::CENTER_CENTER,
                                            orb_labels[k],
                                            FontId::proportional(if is_act { 13.0 } else { 11.0 }),
                                            Color32::WHITE,
                                        );
                                    }
                                    // Full native VSync (60/120Hz) while previewing animations on First Launch
                                    ctx.request_repaint();
                                });
                            });

                        ui.add_space(10.0);

                        // Primary Start CTA Button
                        let cta_focused = this.modal_focus == 4;
                        let full_w = ui.available_width();
                        let (cta_rect, cta_resp) =
                            ui.allocate_exact_size(Vec2::new(full_w, 44.0), Sense::click());
                        let p = ui.painter();
                        p.rect_filled(
                            cta_rect,
                            CornerRadius::same(14),
                            if cta_focused {
                                accent
                            } else {
                                Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 75)
                            },
                        );
                        if cta_focused {
                            p.rect_stroke(
                                cta_rect,
                                CornerRadius::same(14),
                                Stroke::new(2.2_f32, Color32::WHITE),
                                egui::StrokeKind::Inside,
                            );
                        }
                        p.text(
                            cta_rect.center(),
                            Align2::CENTER_CENTER,
                            "Start Astryx Launcher  ›",
                            FontId::proportional(15.5),
                            if cta_focused {
                                Color32::from_rgb(10, 14, 24)
                            } else {
                                Color32::WHITE
                            },
                        );
                        if cta_resp.clicked() {
                            clicked = Some(4);
                        }

                        if let Some(cidx) = clicked {
                            apply_welcome_card(this, cidx, ctx);
                        }
                    },
                );
            }
            ActiveModal::Weather => {
                // D-pad shortcuts inside Weather modal: Left/Right switch °C/°F or Refresh
                let mut toggle_unit = false;
                let mut do_refresh = false;
                if allow_keys {
                    ctx.input(|i| {
                        if i.key_pressed(Key::ArrowLeft) {
                            self.modal_focus = 0;
                        } else if i.key_pressed(Key::ArrowRight) {
                            self.modal_focus = 1;
                        } else if i.key_pressed(Key::Enter) || i.key_pressed(Key::Space) {
                            if self.modal_focus == 0 {
                                toggle_unit = true;
                            } else {
                                do_refresh = true;
                            }
                        }
                    });
                }
                if toggle_unit {
                    self.use_fahrenheit = !self.use_fahrenheit;
                }
                if do_refresh {
                    self.spawn_load_weather(ctx.clone());
                }

                self.draw_modal_shell(ctx, "London Weather Forecast", 780.0, 440.0, |this, ui, ctx| {
                    let accent = this.config.accent();
                    ui.horizontal(|ui| {
                        let unit_label = if this.use_fahrenheit {
                            "Switch to °C"
                        } else {
                            "Switch to °F"
                        };
                        if Self::modal_btn(ui, unit_label, false, this.modal_focus == 0, accent) {
                            this.use_fahrenheit = !this.use_fahrenheit;
                        }
                        if Self::modal_btn(ui, "↻ Refresh", false, this.modal_focus == 1, accent) {
                            this.spawn_load_weather(ctx.clone());
                        }
                    });
                    ui.add_space(10.0);

                    if let Some(w) = this.weather.clone() {
                        // Current summary row with vector weather icon
                        ui.horizontal(|ui| {
                            let (wr, _) = ui.allocate_exact_size(Vec2::splat(40.0), Sense::hover());
                            Self::draw_weather_icon(ui.painter(), wr.center(), w.condition, 1.5);
                            ui.add_space(6.0);
                            ui.label(
                                RichText::new(this.fmt_temp(w.temp_c))
                                    .font(FontId::proportional(38.0))
                                    .strong()
                                    .color(Color32::WHITE),
                            );
                            ui.add_space(12.0);
                            ui.vertical(|ui| {
                                ui.label(
                                    RichText::new(format!("{}  •  Feels like {}", w.condition, this.fmt_temp(w.feels_like_c)))
                                        .font(FontId::proportional(15.0))
                                        .strong()
                                        .color(Color32::WHITE),
                                );
                                ui.label(
                                    RichText::new(format!(
                                        "Humidity {}%  •  Wind {} km/h  •  Rain {}%  •  Sunrise {}  •  Sunset {}",
                                        w.humidity, w.wind_kmh, w.precip_prob, w.sunrise, w.sunset
                                    ))
                                    .font(FontId::proportional(12.5))
                                    .color(Color32::from_rgb(200, 214, 236)),
                                );
                            });
                        });

                        ui.add_space(12.0);
                        ui.label(
                            RichText::new("NEXT 8 HOURS")
                                .font(FontId::proportional(11.5))
                                .strong()
                                .color(accent),
                        );
                        ui.add_space(4.0);
                        ui.columns(w.hourly.len().clamp(1, 8), |cols| {
                            for (i, h) in w.hourly.iter().take(8).enumerate() {
                                cols[i].vertical_centered(|ui| {
                                    egui::Frame::NONE
                                        .fill(Color32::from_rgba_unmultiplied(255, 255, 255, 14))
                                        .corner_radius(CornerRadius::same(10))
                                        .inner_margin(egui::Margin::symmetric(4, 8))
                                        .show(ui, |ui| {
                                            ui.label(
                                                RichText::new(&h.hour)
                                                    .font(FontId::proportional(11.5))
                                                    .color(Color32::from_rgb(200, 214, 235)),
                                            );
                                            ui.label(
                                                RichText::new(this.fmt_temp(h.temp_c))
                                                    .font(FontId::proportional(15.0))
                                                    .strong()
                                                    .color(Color32::WHITE),
                                            );
                                            ui.label(
                                                RichText::new(format!("{}%", h.precip_prob))
                                                    .font(FontId::proportional(11.0))
                                                    .color(accent),
                                            );
                                        });
                                });
                            }
                        });

                        ui.add_space(12.0);
                        ui.label(
                            RichText::new("5-DAY FORECAST")
                                .font(FontId::proportional(11.5))
                                .strong()
                                .color(accent),
                        );
                        ui.add_space(4.0);
                        ui.columns(w.daily.len().clamp(1, 5), |cols| {
                            for (i, d) in w.daily.iter().take(5).enumerate() {
                                cols[i].vertical_centered(|ui| {
                                    egui::Frame::NONE
                                        .fill(Color32::from_rgba_unmultiplied(255, 255, 255, 16))
                                        .corner_radius(CornerRadius::same(12))
                                        .inner_margin(egui::Margin::symmetric(6, 10))
                                        .show(ui, |ui| {
                                            ui.label(
                                                RichText::new(&d.day)
                                                    .font(FontId::proportional(13.0))
                                                    .strong()
                                                    .color(Color32::WHITE),
                                            );
                                            let (dr, _) = ui.allocate_exact_size(Vec2::splat(24.0), Sense::hover());
                                            Self::draw_weather_icon(ui.painter(), dr.center(), d.condition, 0.95);
                                            ui.label(
                                                RichText::new(d.condition)
                                                    .font(FontId::proportional(11.0))
                                                    .color(Color32::from_rgb(195, 210, 232)),
                                            );
                                            ui.label(
                                                RichText::new(format!(
                                                    "{} / {}",
                                                    this.fmt_temp(d.high_c),
                                                    this.fmt_temp(d.low_c)
                                                ))
                                                .font(FontId::proportional(14.0))
                                                .strong()
                                                .color(Color32::WHITE),
                                            );
                                            ui.label(
                                                RichText::new(format!("Rain {}%", d.precip_prob))
                                                    .font(FontId::proportional(11.0))
                                                    .color(accent),
                                            );
                                        });
                                });
                            }
                        });
                    } else {
                        ui.label("Fetching live London forecast from Open-Meteo...");
                    }
                });
            }

            ActiveModal::Theme => {
                // 9 options (perfect 3x3 Glass Card Grid in Grid mode, or grouped rows in List mode!)
                const THEME_COUNT: usize = 9;
                const THEME_COLS: usize = 3;
                let is_grid = self.config.settings_view == SettingsViewMode::Grid;
                let mut step_dir: i8 = 0;

                if allow_keys {
                    ctx.input(|i| {
                        let up = i.key_pressed(Key::ArrowUp);
                        let down = i.key_pressed(Key::ArrowDown);
                        let left = i.key_pressed(Key::ArrowLeft);
                        let right = i.key_pressed(Key::ArrowRight);
                        let ok = i.key_pressed(Key::Enter) || i.key_pressed(Key::Space);

                        if is_grid {
                            let col = self.modal_focus % THEME_COLS;
                            if left && col > 0 {
                                self.modal_focus -= 1;
                                self.modal_focus_changed = true;
                            } else if right && col + 1 < THEME_COLS && self.modal_focus + 1 < THEME_COUNT {
                                self.modal_focus += 1;
                                self.modal_focus_changed = true;
                            } else if up {
                                if self.modal_focus >= THEME_COLS {
                                    self.modal_focus -= THEME_COLS;
                                    self.modal_focus_changed = true;
                                } else {
                                    self.modal_close_focused = true;
                                }
                            } else if down && self.modal_focus + THEME_COLS < THEME_COUNT {
                                self.modal_focus += THEME_COLS;
                                self.modal_focus_changed = true;
                            } else if ok {
                                step_dir = 1;
                            }
                        } else {
                            if down {
                                self.modal_focus = (self.modal_focus + 1).min(THEME_COUNT - 1);
                                self.modal_focus_changed = true;
                            } else if up {
                                if self.modal_focus == 0 {
                                    self.modal_close_focused = true;
                                } else {
                                    self.modal_focus -= 1;
                                    self.modal_focus_changed = true;
                                }
                            } else if ok || right {
                                step_dir = 1;
                            } else if left {
                                step_dir = -1;
                            }
                        }
                    });
                }

                let apply_theme_step = |this: &mut Self, row: usize, dir: i8, ctx: &egui::Context| {
                    match row {
                        0 => {
                            let len = AstryxThemeId::ALL.len();
                            let idx = AstryxThemeId::ALL
                                .iter()
                                .position(|t| *t == this.config.astryx_theme)
                                .unwrap_or(0);
                            let next = if dir >= 0 { (idx + 1) % len } else { (idx + len - 1) % len };
                            this.config.astryx_theme = AstryxThemeId::ALL[next];
                            this.save_config();
                        }
                        1 => {
                            let len = WeatherScene::ALL.len();
                            let idx = WeatherScene::ALL
                                .iter()
                                .position(|s| *s == this.config.weather_scene)
                                .unwrap_or(0);
                            let next = if dir >= 0 { (idx + 1) % len } else { (idx + len - 1) % len };
                            this.config.weather_scene = WeatherScene::ALL[next];
                            this.save_config();
                            this.ensure_wallpaper(ctx.clone());
                        }
                        2 => {
                            let len = ACCENT_PRESETS.len();
                            this.config.accent_idx = if dir >= 0 {
                                (this.config.accent_idx + 1) % len
                            } else {
                                (this.config.accent_idx + len - 1) % len
                            };
                            this.save_config();
                        }
                        3 => {
                            let len = FocusAnimStyle::ALL.len();
                            let idx = FocusAnimStyle::ALL
                                .iter()
                                .position(|a| *a == this.config.focus_anim)
                                .unwrap_or(0);
                            let next = if dir >= 0 { (idx + 1) % len } else { (idx + len - 1) % len };
                            this.config.focus_anim = FocusAnimStyle::ALL[next];
                            this.save_config();
                        }
                        4 => {
                            this.config.modal_pop_in = !this.config.modal_pop_in;
                            this.save_config();
                        }
                        5 => {
                            this.config.menu_transparency = !this.config.menu_transparency;
                            this.save_config();
                        }
                        6 => {
                            if dir >= 0 {
                                // OK / Right cycles cleanly through opacity presets (wraps back to 45%)
                                let cur = (this.config.menu_opacity * 100.0).round() as i32;
                                this.config.menu_opacity = if cur < 55 {
                                    0.60
                                } else if cur < 70 {
                                    0.72
                                } else if cur < 82 {
                                    0.85
                                } else if cur < 92 {
                                    0.95
                                } else {
                                    0.45
                                };
                            } else {
                                this.config.menu_opacity =
                                    (this.config.menu_opacity - 0.08).clamp(0.35, 0.96);
                            }
                            this.save_config();
                        }
                        7 => {
                            this.config.bg_blur = if dir >= 0 {
                                (this.config.bg_blur + 1) % 6
                            } else {
                                (this.config.bg_blur + 5) % 6
                            };
                            this.save_config();
                            this.ensure_wallpaper(ctx.clone());
                        }
                        8 => {
                            this.config.live_sky_motion = !this.config.live_sky_motion;
                            this.save_config();
                        }
                        _ => {}
                    }
                };

                if step_dir != 0 {
                    apply_theme_step(self, self.modal_focus, step_dir, ctx);
                }

                let theme_title = if is_grid {
                    "Theme & Glass Customiser  ·  D-Pad navigate · OK change"
                } else {
                    "Theme & Glass Customiser  ·  Up/Down move · OK change"
                };

                self.draw_modal_shell(
                    ctx,
                    theme_title,
                    820.0,
                    440.0,
                    |this, ui, ctx| {
                        let accent = this.config.accent();
                        let accent_name =
                            ACCENT_PRESETS[this.config.accent_idx % ACCENT_PRESETS.len()].0;

                        let rows: [(&str, String, bool); 9] = [
                            ("Astryx Theme", this.config.astryx_theme.label().to_string(), true),
                            ("Live Wallpaper", this.config.weather_scene.label().to_string(), true),
                            ("Accent Color", accent_name.to_string(), true),
                            ("60fps Focus Anim", this.config.focus_anim.label().to_string(), true),
                            (
                                "Modal Pop-In",
                                if this.config.modal_pop_in { "ON (85ms)" } else { "OFF" }.to_string(),
                                this.config.modal_pop_in,
                            ),
                            (
                                "Menu Transparency",
                                if this.config.menu_transparency { "ON" } else { "OFF" }.to_string(),
                                this.config.menu_transparency,
                            ),
                            (
                                "Menu Opacity",
                                format!("{}%", (this.config.menu_opacity * 100.0).round() as i32),
                                true,
                            ),
                            (
                                "Background Blur",
                                if this.config.bg_blur == 0 {
                                    "OFF (HD)".to_string()
                                } else {
                                    format!("Level {}/5", this.config.bg_blur)
                                },
                                this.config.bg_blur > 0,
                            ),
                            (
                                "Real-Time Sky",
                                if this.config.live_sky_motion { "ON" } else { "OFF" }.to_string(),
                                this.config.live_sky_motion,
                            ),
                        ];

                        let mut clicked_row: Option<usize> = None;

                        if this.config.settings_view == SettingsViewMode::Grid {
                            // Perfect 3x3 Glass Card Grid — all 9 options fit on 1 screen with zero scrolling!
                            for (row_i, chunk) in rows.chunks(THEME_COLS).enumerate() {
                                ui.columns(THEME_COLS, |cols| {
                                    for (c_i, (label, val, is_on)) in chunk.iter().enumerate() {
                                        let idx = row_i * THEME_COLS + c_i;
                                        let focused = this.modal_focus == idx;
                                        let w = cols[c_i].available_width();
                                        let (rect, resp) = cols[c_i]
                                            .allocate_exact_size(Vec2::new(w, 86.0), Sense::click());
                                        let p = cols[c_i].painter();

                                        p.rect_filled(
                                            rect,
                                            CornerRadius::same(14),
                                            if focused {
                                                Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 50)
                                            } else {
                                                Color32::from_rgba_unmultiplied(255, 255, 255, 11)
                                            },
                                        );
                                        p.rect_stroke(
                                            rect,
                                            CornerRadius::same(14),
                                            if focused {
                                                Stroke::new(2.0_f32, accent)
                                            } else {
                                                Stroke::new(
                                                    1.0_f32,
                                                    Color32::from_rgba_unmultiplied(255, 255, 255, 22),
                                                )
                                            },
                                            egui::StrokeKind::Inside,
                                        );

                                        // Top-left frosted circle with color-coded vector icon inside
                                        let ic_c = Pos2::new(rect.left() + 26.0, rect.top() + 26.0);
                                        let tc = Self::theme_icon_color(idx, accent);
                                        p.circle_filled(
                                            ic_c,
                                            13.5,
                                            if focused {
                                                Color32::from_rgba_unmultiplied(255, 255, 255, 34)
                                            } else {
                                                Color32::from_rgba_unmultiplied(255, 255, 255, 18)
                                            },
                                        );
                                        Self::draw_theme_icon(p, ic_c, idx, tc, accent);

                                        // Top-right color-coded value pill
                                        let pill_w = (val.len() as f32 * 6.8 + 20.0).clamp(52.0, 128.0);
                                        let pill_rect = Rect::from_min_size(
                                            Pos2::new(rect.right() - pill_w - 12.0, rect.top() + 14.0),
                                            Vec2::new(pill_w, 24.0),
                                        );
                                        p.rect_filled(
                                            pill_rect,
                                            CornerRadius::same(8),
                                            if focused {
                                                Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 120)
                                            } else if *is_on {
                                                Color32::from_rgba_unmultiplied(52, 211, 153, 42)
                                            } else {
                                                Color32::from_rgba_unmultiplied(255, 255, 255, 16)
                                            },
                                        );
                                        p.text(
                                            pill_rect.center(),
                                            Align2::CENTER_CENTER,
                                            val,
                                            FontId::proportional(11.5),
                                            Color32::WHITE,
                                        );

                                        // Bottom-left bold title
                                        p.text(
                                            Pos2::new(rect.left() + 14.0, rect.bottom() - 18.0),
                                            Align2::LEFT_CENTER,
                                            *label,
                                            FontId::proportional(13.5),
                                            if focused { Color32::WHITE } else { Color32::from_rgb(225, 233, 246) },
                                        );

                                        if resp.clicked() {
                                            this.modal_focus = idx;
                                            clicked_row = Some(idx);
                                        }
                                    }
                                });
                                ui.add_space(10.0);
                            }
                        } else {
                        for (idx, (label, val, is_on)) in rows.iter().enumerate() {
                            let section = match idx {
                                0 => Some("THEME & LIVE WALLPAPER"),
                                3 => Some("60FPS ANIMATIONS"),
                                5 => Some("GLASS, BLUR & MOTION"),
                                _ => None,
                            };
                            if let Some(sec) = section {
                                if idx > 0 {
                                    ui.add_space(6.0);
                                }
                                ui.label(
                                    RichText::new(sec)
                                        .font(FontId::proportional(10.5))
                                        .strong()
                                        .color(accent),
                                );
                                ui.add_space(3.0);
                            }

                            let focused = this.modal_focus == idx;
                            let row_resp = egui::Frame::NONE
                                .fill(if focused {
                                    Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 38)
                                } else {
                                    Color32::from_rgba_unmultiplied(255, 255, 255, 9)
                                })
                                .stroke(if focused {
                                    Stroke::new(1.5_f32, accent)
                                } else {
                                    Stroke::new(
                                        1.0_f32,
                                        Color32::from_rgba_unmultiplied(255, 255, 255, 16),
                                    )
                                })
                                .corner_radius(CornerRadius::same(10))
                                .inner_margin(egui::Margin::symmetric(12, 6))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        let (dot_r, _) =
                                            ui.allocate_exact_size(Vec2::new(5.0, 22.0), Sense::hover());
                                        if focused {
                                            ui.painter().rect_filled(
                                                Rect::from_center_size(dot_r.center(), Vec2::new(3.5, 18.0)),
                                                CornerRadius::same(2),
                                                accent,
                                            );
                                        }
                                        let (ic_r, _) =
                                            ui.allocate_exact_size(Vec2::splat(24.0), Sense::hover());
                                        let tc = Self::theme_icon_color(idx, accent);
                                        ui.painter().circle_filled(
                                            ic_r.center(),
                                            11.5,
                                            if focused {
                                                Color32::from_rgba_unmultiplied(255, 255, 255, 32)
                                            } else {
                                                Color32::from_rgba_unmultiplied(255, 255, 255, 16)
                                            },
                                        );
                                        Self::draw_theme_icon(ui.painter(), ic_r.center(), idx, tc, accent);
                                        ui.add_space(4.0);
                                        ui.label(
                                            RichText::new(*label)
                                                .font(FontId::proportional(13.5))
                                                .strong()
                                                .color(if focused {
                                                    Color32::WHITE
                                                } else {
                                                    Color32::from_rgb(225, 233, 246)
                                                }),
                                        );
                                        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                                            let pill_fill = if focused {
                                                Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 95)
                                            } else if *is_on {
                                                Color32::from_rgba_unmultiplied(52, 211, 153, 38)
                                            } else {
                                                Color32::from_rgba_unmultiplied(255, 255, 255, 14)
                                            };
                                            let pill_stroke = if focused {
                                                Stroke::new(1.6_f32, Color32::WHITE)
                                            } else if *is_on {
                                                Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(52, 211, 153, 120))
                                            } else {
                                                Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 24))
                                            };
                                            if ui
                                                .add(
                                                    egui::Button::new(
                                                        RichText::new(val)
                                                            .font(FontId::proportional(12.0))
                                                            .strong()
                                                            .color(Color32::WHITE),
                                                    )
                                                    .fill(pill_fill)
                                                    .stroke(pill_stroke)
                                                    .corner_radius(CornerRadius::same(8))
                                                    .min_size(Vec2::new(136.0, 26.0)),
                                                )
                                                .clicked()
                                            {
                                                this.modal_focus = idx;
                                                clicked_row = Some(idx);
                                            }
                                        });
                                    });
                                })
                                .response;
                            if focused && this.modal_focus_changed {
                                row_resp.scroll_to_me(Some(Align::Center));
                                this.modal_focus_changed = false;
                            }
                            ui.add_space(3.0);
                        }
                        }
                        if let Some(r) = clicked_row {
                            apply_theme_step(this, r, 1, ctx);
                        }
                    },
                );
            }

            ActiveModal::Settings => {
                // =====================================================================
                // Option 04: Cinema Ribbon & Live Preview Settings Studio
                //   Top floating 5-tab Cinema Ribbon + 60/40 Split Glass Stage
                // =====================================================================
                let jni_back = bridge::take_back_pressed();
                let key_back =
                    ctx.input(|i| i.key_pressed(Key::Escape) || i.key_pressed(Key::Backspace));
                if (jni_back || key_back)
                    && self.last_back_handled.elapsed() > Duration::from_millis(260)
                {
                    self.last_back_handled = Instant::now();
                    self.close_modal_step();
                    return;
                }

                // 5 Ribbon Categories: (label, icon_id)
                const TABS: [(&str, usize); 5] = [
                    ("Appearance", 201),
                    ("Weather & HUD", 4),
                    ("Gemini AI", 203),
                    ("Apps", 2),
                    ("System", 204),
                ];

                // Kind: 0 = bool toggle pill, 1 = cycle value badge, 2 = submenu / action chevron
                struct SettingRow {
                    label: &'static str,
                    desc: &'static str,
                    val: String,
                    is_on: bool,
                    icon_id: usize,
                    kind: u8,
                }

                self.settings_tab = self.settings_tab.min(TABS.len() - 1);
                let tab = self.settings_tab;
                let has_key = !self.config.gemini_api_key.trim().is_empty();

                let rows: Vec<SettingRow> = match tab {
                    // Tab 0: Appearance (9 items)
                    0 => vec![
                        SettingRow {
                            label: "Theme & Glass",
                            desc: "Customize surface tint, 4K live weather or shader wallpaper, accent hue, and frosted glass blur.",
                            val: self.config.astryx_theme.label().replace("Astryx ", ""),
                            is_on: true,
                            icon_id: 201,
                            kind: 2,
                        },
                        SettingRow {
                            label: "Home Layout",
                            desc: "Switch between Zenith Horizon floating bottom dock and Classic full-screen App Grid.",
                            val: self.config.layout_style.label().to_string(),
                            is_on: true,
                            icon_id: 0,
                            kind: 1,
                        },
                        SettingRow {
                            label: "Screensaver Style",
                            desc: "Pick the centered ambient clock aesthetic shown when your TV sits idle.",
                            val: self.config.screensaver_style.label().to_string(),
                            is_on: self.config.ambient_timeout_secs > 0,
                            icon_id: 8,
                            kind: 1,
                        },
                        SettingRow {
                            label: "Ambient Clock Timer",
                            desc: "Idle timeout before Astryx dims the home grid into the static centered screensaver.",
                            val: match self.config.ambient_timeout_secs {
                                0 => "OFF".to_string(),
                                60 => "1 min".to_string(),
                                180 => "3 min".to_string(),
                                _ => "5 min".to_string(),
                            },
                            is_on: self.config.ambient_timeout_secs > 0,
                            icon_id: 3,
                            kind: 1,
                        },
                        SettingRow {
                            label: "App Icon Size",
                            desc: "Scale home screen app tiles for comfortable 10-foot living-room readability.",
                            val: self.config.icon_size.label().to_string(),
                            is_on: true,
                            icon_id: 1,
                            kind: 1,
                        },
                        SettingRow {
                            label: "Clock & Weather Header",
                            desc: "Display live time, date, and weather headline in the top-left of the home screen.",
                            val: if self.config.show_header { "ON" } else { "OFF" }.to_string(),
                            is_on: self.config.show_header,
                            icon_id: 3,
                            kind: 0,
                        },
                        SettingRow {
                            label: "Show App Labels",
                            desc: "Display app names underneath each icon tile on the home screen.",
                            val: if self.config.show_labels { "ON" } else { "OFF" }.to_string(),
                            is_on: self.config.show_labels,
                            icon_id: 5,
                            kind: 0,
                        },
                        SettingRow {
                            label: "Auto Night Dimming",
                            desc: "Gently dim background wallpaper luminance during late evening viewing hours.",
                            val: if self.config.auto_night_dim { "ON" } else { "OFF" }.to_string(),
                            is_on: self.config.auto_night_dim,
                            icon_id: 8,
                            kind: 0,
                        },
                        SettingRow {
                            label: "Lightweight Mode",
                            desc: "Reduce tessellation detail and shader glow for lowest CPU/VRAM usage on older TVs.",
                            val: if self.config.lightweight_mode { "ON" } else { "OFF" }.to_string(),
                            is_on: self.config.lightweight_mode,
                            icon_id: 14,
                            kind: 0,
                        },
                    ],
                    // Tab 1: Weather & HUD (6 items)
                    1 => vec![
                        SettingRow {
                            label: "Live Weather Forecast",
                            desc: "Fetch and show real-time temperature and sky conditions in the header and ambient clock.",
                            val: if self.config.show_weather { "ON" } else { "OFF" }.to_string(),
                            is_on: self.config.show_weather,
                            icon_id: 4,
                            kind: 0,
                        },
                        SettingRow {
                            label: "Weather City",
                            desc: "Choose the city coordinates used for local forecast and temperature readouts.",
                            val: self
                                .config
                                .weather_city
                                .split('·')
                                .next()
                                .unwrap_or("London")
                                .trim()
                                .to_string(),
                            is_on: true,
                            icon_id: 4,
                            kind: 2,
                        },
                        SettingRow {
                            label: "24-Hour UK Clock",
                            desc: "Display 24-hour time (21:42) instead of 12-hour AM/PM formatting.",
                            val: if self.config.clock_24h { "24h" } else { "12h" }.to_string(),
                            is_on: self.config.clock_24h,
                            icon_id: 6,
                            kind: 0,
                        },
                        SettingRow {
                            label: "Live Status Pill",
                            desc: "Show compact Wi-Fi, VPN shield, and free memory indicator on the home screen.",
                            val: if self.config.show_status_pill { "ON" } else { "OFF" }.to_string(),
                            is_on: self.config.show_status_pill,
                            icon_id: 7,
                            kind: 0,
                        },
                        SettingRow {
                            label: "Now Playing Pill",
                            desc: "Surface active music or media playback status directly on the home screen.",
                            val: if self.config.show_now_playing { "ON" } else { "OFF" }.to_string(),
                            is_on: self.config.show_now_playing,
                            icon_id: 7,
                            kind: 0,
                        },
                        SettingRow {
                            label: "Performance HUD Overlay",
                            desc: "Configure the floating FPS, refresh rate, network, RAM, CPU, and GPU telemetry bar.",
                            val: if self.config.show_fps_meter { "ON" } else { "OFF" }.to_string(),
                            is_on: self.config.show_fps_meter,
                            icon_id: 14,
                            kind: 2,
                        },
                    ],
                    // Tab 2: Gemini AI (6 items)
                    2 => vec![
                        SettingRow {
                            label: "Gemini AI & Top-Bar Icon",
                            desc: "Enable the multi-color Gemini star in the home screen dock for instant voice chat.",
                            val: if self.config.gemini_enabled { "ON" } else { "OFF" }.to_string(),
                            is_on: self.config.gemini_enabled,
                            icon_id: 203,
                            kind: 0,
                        },
                        SettingRow {
                            label: "Gemini Model",
                            desc: "Cycle Interactions API models for voice assistant speed vs deep reasoning quality.",
                            val: self.config.gemini_model.clone(),
                            is_on: true,
                            icon_id: 101,
                            kind: 1,
                        },
                        SettingRow {
                            label: "Launch Voice Orb",
                            desc: "Open the floating 60fps liquid-plasma voice orb immediately (press BACK or HOME to exit).",
                            val: "Start".to_string(),
                            is_on: has_key && self.config.gemini_enabled,
                            icon_id: 203,
                            kind: 2,
                        },
                        SettingRow {
                            label: "API Key & Text Prompts",
                            desc: "Open the full Gemini AI sheet for Phone Companion API key setup and typed prompts.",
                            val: if has_key { "Configured" } else { "Set Key" }.to_string(),
                            is_on: has_key,
                            icon_id: 200,
                            kind: 2,
                        },
                        SettingRow {
                            label: "Quick Voice: What to Watch",
                            desc: "Ask Gemini aloud for 3 curated movie or show picks to watch tonight on your TV.",
                            val: "Run".to_string(),
                            is_on: false,
                            icon_id: 13,
                            kind: 2,
                        },
                        SettingRow {
                            label: "Clear Chat & A2UI Surfaces",
                            desc: "Reset Gemini multi-turn conversation memory and remove custom generated A2UI cards.",
                            val: "Reset".to_string(),
                            is_on: false,
                            icon_id: 12,
                            kind: 2,
                        },
                    ],
                    // Tab 3: Apps & Library (6 items)
                    3 => vec![
                        SettingRow {
                            label: "App Sort Order",
                            desc: "Order your app library alphabetically (A–Z) or by most recently launched apps first.",
                            val: match self.config.sort_order {
                                SortOrder::Alpha => "A–Z".to_string(),
                                SortOrder::Recent => "Recent".to_string(),
                            },
                            is_on: true,
                            icon_id: 2,
                            kind: 1,
                        },
                        SettingRow {
                            label: "App Folders",
                            desc: "Organize streaming, gaming, and utility apps into clean custom home folders.",
                            val: format!("{} folders", self.config.app_folders.len()),
                            is_on: !self.config.app_folders.is_empty(),
                            icon_id: 201,
                            kind: 2,
                        },
                        SettingRow {
                            label: "Mobile / Sideloaded Apps",
                            desc: "Include sideloaded phone APKs without Android TV Leanback banners in your library.",
                            val: if self.config.show_mobile_apps { "ON" } else { "OFF" }.to_string(),
                            is_on: self.config.show_mobile_apps,
                            icon_id: 9,
                            kind: 0,
                        },
                        SettingRow {
                            label: "System Apps",
                            desc: "Reveal built-in Android TV system packages alongside your installed apps.",
                            val: if self.config.show_system_apps { "ON" } else { "OFF" }.to_string(),
                            is_on: self.config.show_system_apps,
                            icon_id: 10,
                            kind: 0,
                        },
                        SettingRow {
                            label: "Auto-Clear RAM on Home",
                            desc: "Automatically trim idle background processes whenever you return to Astryx Home.",
                            val: if self.config.auto_clear_ram_on_home { "ON" } else { "OFF" }.to_string(),
                            is_on: self.config.auto_clear_ram_on_home,
                            icon_id: 101,
                            kind: 0,
                        },
                        SettingRow {
                            label: "Restore Hidden Apps",
                            desc: "Unhide any apps you previously concealed from the home screen grid.",
                            val: format!("Unhide ({})", self.config.hidden_apps.len()),
                            is_on: !self.config.hidden_apps.is_empty(),
                            icon_id: 12,
                            kind: 2,
                        },
                    ],
                    // Tab 4: System & Power (7 items)
                    _ => vec![
                        SettingRow {
                            label: "Startup Options",
                            desc: "Choose whether booting stays on Astryx Home, launches an app, or switches HDMI input.",
                            val: self.config.startup_mode.label().to_string(),
                            is_on: self.config.startup_mode != StartupMode::Home,
                            icon_id: 100,
                            kind: 2,
                        },
                        SettingRow {
                            label: "Default Home Launcher",
                            desc: "Open Android TV Home App picker to verify or set Astryx as your default launcher.",
                            val: if self.is_default_home {
                                "Active".to_string()
                            } else {
                                "Set Default".to_string()
                            },
                            is_on: self.is_default_home,
                            icon_id: 11,
                            kind: 2,
                        },
                        SettingRow {
                            label: "Power & Devices",
                            desc: "Access TV sleep, reboot, Bluetooth remotes, and connected display device controls.",
                            val: "BT · Sleep".to_string(),
                            is_on: false,
                            icon_id: 100,
                            kind: 2,
                        },
                        SettingRow {
                            label: "Phone Companion",
                            desc: "Control settings, upload 4K wallpapers, paste API keys, or talk to Gemini from your phone.",
                            val: format!(":{}", WEB_COMPANION_PORT),
                            is_on: self.config.web_companion_enabled,
                            icon_id: 200,
                            kind: 2,
                        },
                        SettingRow {
                            label: "Backup & Restore",
                            desc: "Export or import your full Astryx launcher configuration as a portable JSON snapshot.",
                            val: "JSON".to_string(),
                            is_on: false,
                            icon_id: 102,
                            kind: 2,
                        },
                        SettingRow {
                            label: "App Updates",
                            desc: "Check GitHub Releases for newer Astryx Rust Launcher builds and install OTA.",
                            val: match &self.updater_state {
                                UpdaterState::Available(r) => format!("v{} NEW", r.version),
                                UpdaterState::Downloading { .. } => "Downloading".to_string(),
                                UpdaterState::ReadyToInstall { .. }
                                | UpdaterState::NeedPermission { .. } => "Install".to_string(),
                                UpdaterState::Checking => "Checking...".to_string(),
                                _ => format!("v{}", self.app_version),
                            },
                            is_on: self.has_update_available(),
                            icon_id: 13,
                            kind: 2,
                        },
                        SettingRow {
                            label: "Buy Me a Coffee",
                            desc: "Support independent pure-Rust Android TV development via QR code.",
                            val: "Support".to_string(),
                            is_on: false,
                            icon_id: 15,
                            kind: 2,
                        },
                    ],
                };

                let row_count = rows.len();
                if self.modal_focus >= row_count {
                    self.modal_focus = row_count.saturating_sub(1);
                }

                // D-Pad Input Handling across Ribbon / List / QuickBar
                let mut activate_list: Option<(usize, usize)> = None;
                let mut activate_quick: Option<usize> = None;
                let mut close_now = false;

                if allow_keys {
                    ctx.input(|i| {
                        let up = i.key_pressed(Key::ArrowUp);
                        let down = i.key_pressed(Key::ArrowDown);
                        let left = i.key_pressed(Key::ArrowLeft);
                        let right = i.key_pressed(Key::ArrowRight);
                        let ok = i.key_pressed(Key::Enter) || i.key_pressed(Key::Space);

                        match self.settings_zone {
                            SettingsZone::Ribbon => {
                                if left {
                                    self.settings_ribbon_focus =
                                        self.settings_ribbon_focus.saturating_sub(1);
                                    if self.settings_ribbon_focus < TABS.len() {
                                        self.settings_tab = self.settings_ribbon_focus;
                                        self.modal_focus = 0;
                                        self.modal_focus_changed = true;
                                    }
                                } else if right {
                                    self.settings_ribbon_focus =
                                        (self.settings_ribbon_focus + 1).min(TABS.len());
                                    if self.settings_ribbon_focus < TABS.len() {
                                        self.settings_tab = self.settings_ribbon_focus;
                                        self.modal_focus = 0;
                                        self.modal_focus_changed = true;
                                    }
                                } else if down {
                                    self.settings_zone = SettingsZone::List;
                                    self.modal_focus_changed = true;
                                } else if ok {
                                    if self.settings_ribbon_focus == TABS.len() {
                                        close_now = true;
                                    } else {
                                        self.settings_tab = self.settings_ribbon_focus;
                                        self.settings_zone = SettingsZone::List;
                                        self.modal_focus_changed = true;
                                    }
                                }
                            }
                            SettingsZone::List => {
                                if up {
                                    if self.modal_focus == 0 {
                                        self.settings_zone = SettingsZone::Ribbon;
                                        self.settings_ribbon_focus = self.settings_tab;
                                    } else {
                                        self.modal_focus -= 1;
                                        self.modal_focus_changed = true;
                                    }
                                } else if down {
                                    if self.modal_focus + 1 < row_count {
                                        self.modal_focus += 1;
                                        self.modal_focus_changed = true;
                                    }
                                } else if left {
                                    self.settings_zone = SettingsZone::Ribbon;
                                    self.settings_ribbon_focus = self.settings_tab;
                                } else if right {
                                    self.settings_zone = SettingsZone::QuickBar;
                                    self.settings_sys_col = 0;
                                } else if ok {
                                    activate_list = Some((self.settings_tab, self.modal_focus));
                                }
                            }
                            SettingsZone::QuickBar => {
                                if self.settings_sys_col < 4 {
                                    if left {
                                        if self.settings_sys_col == 0 {
                                            self.settings_zone = SettingsZone::List;
                                        } else {
                                            self.settings_sys_col -= 1;
                                        }
                                    } else if right {
                                        self.settings_sys_col = (self.settings_sys_col + 1).min(3);
                                    } else if down {
                                        self.settings_sys_col = 4;
                                    } else if up {
                                        self.settings_zone = SettingsZone::Ribbon;
                                        self.settings_ribbon_focus = self.settings_tab;
                                    } else if ok {
                                        activate_quick = Some(self.settings_sys_col);
                                    }
                                } else {
                                    // settings_sys_col == 4 (Boost RAM full-width button)
                                    if up {
                                        self.settings_sys_col = 0;
                                    } else if left {
                                        self.settings_zone = SettingsZone::List;
                                    } else if ok {
                                        activate_quick = Some(4);
                                    }
                                }
                            }
                        }
                    });
                }

                if close_now {
                    self.close_modal_step();
                    return;
                }

                // Scrim & smooth modal pop-in scaling
                let screen = ctx.screen_rect();
                let dim = ctx.layer_painter(egui::LayerId::new(
                    egui::Order::Middle,
                    egui::Id::new("settings_ribbon_scrim"),
                ));
                dim.rect_filled(screen, 0.0, Color32::from_rgba_unmultiplied(4, 6, 14, 182));

                let fill = self.config.modal_fill();
                let accent = self.config.accent();
                let pop_scale = if self.config.modal_pop_in && !self.config.lightweight_mode {
                    let el = self.modal_opened_at.elapsed().as_secs_f32();
                    if el < 0.085 {
                        ctx.request_repaint();
                        let frac = (el / 0.085).clamp(0.0, 1.0);
                        let ease = 1.0 - (1.0 - frac).powi(3);
                        0.94 + 0.06 * ease
                    } else {
                        1.0
                    }
                } else {
                    1.0
                };

                let stage_w = ((screen.width() - 44.0).min(912.0)) * pop_scale;
                let stage_h = ((screen.height() - 62.0).min(472.0)) * pop_scale;
                let ribbon_h = 48.0_f32;
                let gap_v = 12.0_f32;
                let gap_h = 12.0_f32;
                let body_h = (stage_h - ribbon_h - gap_v).max(320.0);
                let left_w = ((stage_w - gap_h) * 0.575).floor();
                let right_w = (stage_w - gap_h - left_w).floor();
                let stage_origin = Pos2::new(
                    (screen.center().x - stage_w * 0.5).round(),
                    (screen.center().y - stage_h * 0.5 - 6.0).round().max(14.0),
                );

                let mut clicked_tab: Option<usize> = None;
                let mut clicked_close = false;

                egui::Area::new(egui::Id::new("settings_cinema_stage"))
                    .fixed_pos(stage_origin)
                    .order(egui::Order::Foreground)
                    .show(ctx, |ui| {
                        ui.allocate_ui_with_layout(
                            Vec2::new(stage_w, stage_h),
                            egui::Layout::top_down(Align::LEFT),
                            |ui| {
                                ui.spacing_mut().item_spacing = Vec2::ZERO;

                                // -------------------------------------------------------------
                                // 1. TOP FLOATING CINEMA RIBBON
                                // -------------------------------------------------------------
                                egui::Frame::NONE
                                    .fill(fill)
                                    .stroke(Stroke::new(
                                        1.3_f32,
                                        Color32::from_rgba_unmultiplied(255, 255, 255, 48),
                                    ))
                                    .corner_radius(CornerRadius::same(16))
                                    .inner_margin(egui::Margin::symmetric(14, 7))
                                    .show(ui, |ui| {
                                        ui.allocate_ui_with_layout(
                                            Vec2::new(stage_w - 28.0, ribbon_h - 14.0),
                                            egui::Layout::left_to_right(Align::Center),
                                            |ui| {
                                                // Brand mark
                                                ui.label(
                                                    RichText::new("ASTRYX")
                                                        .font(FontId::proportional(13.5))
                                                        .strong()
                                                        .color(Color32::WHITE),
                                                );
                                                ui.add_space(5.0);
                                                ui.label(
                                                    RichText::new("/ Settings")
                                                        .font(FontId::proportional(12.0))
                                                        .color(Color32::from_rgb(154, 172, 200)),
                                                );
                                                ui.add_space(16.0);

                                                // 5 Category Pill Tabs
                                                let tab_widths = [108.0_f32, 126.0, 100.0, 74.0, 86.0];
                                                for (t_idx, (t_lbl, t_icon)) in TABS.iter().enumerate() {
                                                    let is_active = tab == t_idx;
                                                    let is_focused =
                                                        self.settings_zone == SettingsZone::Ribbon
                                                            && self.settings_ribbon_focus == t_idx;
                                                    let pill_w = tab_widths[t_idx];
                                                    let (r, resp) = ui.allocate_exact_size(
                                                        Vec2::new(pill_w, 31.0),
                                                        Sense::click(),
                                                    );
                                                    let p = ui.painter();
                                                    let bg = if is_focused {
                                                        Color32::from_rgba_unmultiplied(
                                                            accent.r(),
                                                            accent.g(),
                                                            accent.b(),
                                                            96,
                                                        )
                                                    } else if is_active {
                                                        Color32::from_rgba_unmultiplied(
                                                            accent.r(),
                                                            accent.g(),
                                                            accent.b(),
                                                            46,
                                                        )
                                                    } else {
                                                        Color32::from_rgba_unmultiplied(
                                                            255, 255, 255, 8,
                                                        )
                                                    };
                                                    let stroke = if is_focused {
                                                        Stroke::new(2.0_f32, Color32::WHITE)
                                                    } else if is_active {
                                                        Stroke::new(1.3_f32, accent)
                                                    } else {
                                                        Stroke::new(
                                                            1.0_f32,
                                                            Color32::from_rgba_unmultiplied(
                                                                255, 255, 255, 18,
                                                            ),
                                                        )
                                                    };
                                                    p.rect_filled(r, CornerRadius::same(10), bg);
                                                    p.rect_stroke(
                                                        r,
                                                        CornerRadius::same(10),
                                                        stroke,
                                                        egui::StrokeKind::Inside,
                                                    );
                                                    let ic_c =
                                                        Pos2::new(r.left() + 16.0, r.center().y);
                                                    let ic_col = if is_active || is_focused {
                                                        Color32::WHITE
                                                    } else {
                                                        Self::setting_icon_color(*t_icon)
                                                    };
                                                    Self::draw_setting_icon(
                                                        p, ic_c, *t_icon, ic_col,
                                                    );
                                                    p.text(
                                                        Pos2::new(r.left() + 30.0, r.center().y),
                                                        Align2::LEFT_CENTER,
                                                        *t_lbl,
                                                        FontId::proportional(11.8),
                                                        if is_active || is_focused {
                                                            Color32::WHITE
                                                        } else {
                                                            Color32::from_rgb(198, 212, 232)
                                                        },
                                                    );
                                                    if resp.clicked() {
                                                        clicked_tab = Some(t_idx);
                                                    }
                                                    ui.add_space(6.0);
                                                }

                                                // Right side: Live clock + Close (×) button
                                                ui.with_layout(
                                                    egui::Layout::right_to_left(Align::Center),
                                                    |ui| {
                                                        let close_focused =
                                                            self.settings_zone == SettingsZone::Ribbon
                                                                && self.settings_ribbon_focus
                                                                    == TABS.len();
                                                        let (cr, cresp) = ui.allocate_exact_size(
                                                            Vec2::splat(28.0),
                                                            Sense::click(),
                                                        );
                                                        let cp = ui.painter();
                                                        let cc = cr.center();
                                                        cp.circle_filled(
                                                            cc,
                                                            13.0,
                                                            if close_focused {
                                                                accent
                                                            } else {
                                                                Color32::from_rgba_unmultiplied(
                                                                    255, 255, 255, 22,
                                                                )
                                                            },
                                                        );
                                                        cp.circle_stroke(
                                                            cc,
                                                            13.0,
                                                            if close_focused {
                                                                Stroke::new(2.0_f32, Color32::WHITE)
                                                            } else {
                                                                Stroke::new(
                                                                    1.0_f32,
                                                                    Color32::from_rgba_unmultiplied(
                                                                        accent.r(),
                                                                        accent.g(),
                                                                        accent.b(),
                                                                        155,
                                                                    ),
                                                                )
                                                            },
                                                        );
                                                        let xs = Stroke::new(1.9_f32, Color32::WHITE);
                                                        cp.line_segment(
                                                            [
                                                                cc + Vec2::new(-4.2, -4.2),
                                                                cc + Vec2::new(4.2, 4.2),
                                                            ],
                                                            xs,
                                                        );
                                                        cp.line_segment(
                                                            [
                                                                cc + Vec2::new(4.2, -4.2),
                                                                cc + Vec2::new(-4.2, 4.2),
                                                            ],
                                                            xs,
                                                        );
                                                        if cresp.clicked() {
                                                            clicked_close = true;
                                                        }

                                                        ui.add_space(10.0);
                                                        ui.label(
                                                            RichText::new(&self.cached_time_str)
                                                                .font(FontId::proportional(12.0))
                                                                .strong()
                                                                .color(Color32::from_rgb(
                                                                    220, 232, 248,
                                                                )),
                                                        );
                                                        ui.add_space(5.0);
                                                        let (dr, _) = ui.allocate_exact_size(
                                                            Vec2::splat(8.0),
                                                            Sense::hover(),
                                                        );
                                                        ui.painter().circle_filled(
                                                            dr.center(),
                                                            3.2,
                                                            if self.status.is_connected {
                                                                Color32::from_rgb(52, 211, 153)
                                                            } else {
                                                                Color32::from_rgb(248, 113, 113)
                                                            },
                                                        );
                                                    },
                                                );
                                            },
                                        );
                                    });

                                ui.add_space(gap_v);

                                // -------------------------------------------------------------
                                // 2. 60 / 40 SPLIT GLASS STAGE
                                // -------------------------------------------------------------
                                ui.allocate_ui_with_layout(
                                    Vec2::new(stage_w, body_h),
                                    egui::Layout::left_to_right(Align::TOP),
                                    |ui| {
                                        ui.spacing_mut().item_spacing = Vec2::new(gap_h, 0.0);

                                        // LEFT 60%: Clean Single-Column Settings List Card (explicit top-down!)
                                        ui.allocate_ui_with_layout(
                                            Vec2::new(left_w, body_h),
                                            egui::Layout::top_down(Align::LEFT),
                                            |ui| {
                                                egui::Frame::NONE
                                                    .fill(fill)
                                                    .stroke(Stroke::new(
                                                        1.2_f32,
                                                        Color32::from_rgba_unmultiplied(
                                                            255, 255, 255, 42,
                                                        ),
                                                    ))
                                                    .corner_radius(CornerRadius::same(18))
                                                    .inner_margin(egui::Margin::same(12))
                                                    .show(ui, |ui| {
                                                        let list_inner_w = left_w - 24.0;
                                                        let list_inner_h = body_h - 24.0;
                                                        ui.set_min_size(Vec2::new(
                                                            list_inner_w,
                                                            list_inner_h,
                                                        ));
                                                        ui.allocate_ui_with_layout(
                                                            Vec2::new(list_inner_w, list_inner_h),
                                                            egui::Layout::top_down(Align::LEFT),
                                                            |ui| {
                                                                ui.set_min_height(list_inner_h);
                                                                ui.spacing_mut().item_spacing =
                                                                    Vec2::new(6.0, 0.0);

                                                                // Section heading bar
                                                                ui.horizontal(|ui| {
                                                                    let (bar_r, _) = ui
                                                                        .allocate_exact_size(
                                                                            Vec2::new(3.5, 14.0),
                                                                            Sense::hover(),
                                                                        );
                                                                    ui.painter().rect_filled(
                                                                        bar_r,
                                                                        CornerRadius::same(2),
                                                                        accent,
                                                                    );
                                                                    ui.label(
                                                                        RichText::new(
                                                                            TABS[tab]
                                                                                .0
                                                                                .to_uppercase(),
                                                                        )
                                                                        .font(FontId::proportional(
                                                                            11.0,
                                                                        ))
                                                                        .strong()
                                                                        .color(accent),
                                                                    );
                                                                    ui.label(
                                                                        RichText::new(format!(
                                                                            "· {} items",
                                                                            rows.len()
                                                                        ))
                                                                        .font(FontId::proportional(
                                                                            10.5,
                                                                        ))
                                                                        .color(Color32::from_rgb(
                                                                            152, 170, 198,
                                                                        )),
                                                                    );
                                                                    ui.with_layout(
                                                                        egui::Layout::right_to_left(
                                                                            Align::Center,
                                                                        ),
                                                                        |ui| {
                                                                            ui.label(
                                                                                RichText::new(
                                                                                    "UP Tabs  ·  RIGHT Tools",
                                                                                )
                                                                                .font(
                                                                                    FontId::proportional(
                                                                                        10.0,
                                                                                    ),
                                                                                )
                                                                                .color(
                                                                                    Color32::from_rgb(
                                                                                        138, 156,
                                                                                        184,
                                                                                    ),
                                                                                ),
                                                                            );
                                                                        },
                                                                    );
                                                                });
                                                                ui.add_space(8.0);

                                                                ScrollArea::vertical()
                                                                    .id_salt((
                                                                        "cinema_settings_list",
                                                                        tab,
                                                                    ))
                                                                    .max_height(
                                                                        list_inner_h - 26.0,
                                                                    )
                                                                    .auto_shrink([false, false])
                                                                    .show(ui, |ui| {
                                                                        let row_w =
                                                                            ui.available_width();
                                                                        for (idx, row) in
                                                                            rows.iter().enumerate()
                                                                        {
                                                                            let focused =
                                                                                self.settings_zone
                                                                                    == SettingsZone::List
                                                                                    && self.modal_focus
                                                                                        == idx;
                                                                            let selected_dim =
                                                                                self.settings_zone
                                                                                    != SettingsZone::List
                                                                                    && self.modal_focus
                                                                                        == idx;
                                                                            let (rect, resp) = ui
                                                                                .allocate_exact_size(
                                                                                    Vec2::new(
                                                                                        row_w, 45.0,
                                                                                    ),
                                                                                    Sense::click(),
                                                                                );
                                                                            let p = ui.painter();

                                                                            p.rect_filled(
                                                                                rect,
                                                                                CornerRadius::same(
                                                                                    11,
                                                                                ),
                                                                                if focused {
                                                                                    Color32::from_rgba_unmultiplied(
                                                                                        accent.r(),
                                                                                        accent.g(),
                                                                                        accent.b(),
                                                                                        46,
                                                                                    )
                                                                                } else if selected_dim {
                                                                                    Color32::from_rgba_unmultiplied(
                                                                                        accent.r(),
                                                                                        accent.g(),
                                                                                        accent.b(),
                                                                                        22,
                                                                                    )
                                                                                } else {
                                                                                    Color32::from_rgba_unmultiplied(
                                                                                        255, 255,
                                                                                        255, 10,
                                                                                    )
                                                                                },
                                                                            );
                                                                            p.rect_stroke(
                                                                                rect,
                                                                                CornerRadius::same(
                                                                                    11,
                                                                                ),
                                                                                if focused {
                                                                                    Stroke::new(
                                                                                        1.8_f32,
                                                                                        accent,
                                                                                    )
                                                                                } else if selected_dim {
                                                                                    Stroke::new(
                                                                                        1.1_f32,
                                                                                        Color32::from_rgba_unmultiplied(
                                                                                            accent.r(),
                                                                                            accent.g(),
                                                                                            accent.b(),
                                                                                            100,
                                                                                        ),
                                                                                    )
                                                                                } else {
                                                                                    Stroke::new(
                                                                                        1.0_f32,
                                                                                        Color32::from_rgba_unmultiplied(
                                                                                            255, 255,
                                                                                            255, 18,
                                                                                        ),
                                                                                    )
                                                                                },
                                                                                egui::StrokeKind::Inside,
                                                                            );

                                                                            if focused {
                                                                                p.rect_filled(
                                                                                    Rect::from_center_size(
                                                                                        Pos2::new(
                                                                                            rect.left()
                                                                                                + 6.0,
                                                                                            rect.center()
                                                                                                .y,
                                                                                        ),
                                                                                        Vec2::new(
                                                                                            3.5,
                                                                                            24.0,
                                                                                        ),
                                                                                    ),
                                                                                    CornerRadius::same(
                                                                                        2,
                                                                                    ),
                                                                                    accent,
                                                                                );
                                                                            }

                                                                            let ic_c = Pos2::new(
                                                                                rect.left() + 26.0,
                                                                                rect.center().y,
                                                                            );
                                                                            let sc =
                                                                                Self::setting_icon_color(
                                                                                    row.icon_id,
                                                                                );
                                                                            p.circle_filled(
                                                                                ic_c,
                                                                                12.0,
                                                                                if focused {
                                                                                    Color32::from_rgba_unmultiplied(
                                                                                        255, 255,
                                                                                        255, 34,
                                                                                    )
                                                                                } else {
                                                                                    Color32::from_rgba_unmultiplied(
                                                                                        255, 255,
                                                                                        255, 16,
                                                                                    )
                                                                                },
                                                                            );
                                                                            Self::draw_setting_icon(
                                                                                p,
                                                                                ic_c,
                                                                                row.icon_id,
                                                                                sc,
                                                                            );

                                                                            p.text(
                                                                                Pos2::new(
                                                                                    rect.left()
                                                                                        + 48.0,
                                                                                    rect.center().y,
                                                                                ),
                                                                                Align2::LEFT_CENTER,
                                                                                row.label,
                                                                                FontId::proportional(
                                                                                    13.5,
                                                                                ),
                                                                                if focused {
                                                                                    Color32::WHITE
                                                                                } else {
                                                                                    Color32::from_rgb(
                                                                                        225, 234,
                                                                                        248,
                                                                                    )
                                                                                },
                                                                            );

                                                                            match row.kind {
                                                                                0 => {
                                                                                    let pr = Rect::from_center_size(
                                                                                        Pos2::new(
                                                                                            rect.right() - 32.0,
                                                                                            rect.center().y,
                                                                                        ),
                                                                                        Vec2::new(40.0, 21.0),
                                                                                    );
                                                                                    Self::draw_toggle_pill(
                                                                                        ctx,
                                                                                        p,
                                                                                        pr,
                                                                                        row.is_on,
                                                                                        focused,
                                                                                        accent,
                                                                                        tab * 32 + idx,
                                                                                    );
                                                                                }
                                                                                1 => {
                                                                                    let pw = (row.val.len() as f32 * 7.2 + 22.0)
                                                                                        .clamp(64.0, 148.0);
                                                                                    let pr = Rect::from_center_size(
                                                                                        Pos2::new(
                                                                                            rect.right() - 12.0 - pw * 0.5,
                                                                                            rect.center().y,
                                                                                        ),
                                                                                        Vec2::new(pw, 24.0),
                                                                                    );
                                                                                    p.rect_filled(
                                                                                        pr,
                                                                                        CornerRadius::same(8),
                                                                                        if focused {
                                                                                            Color32::from_rgba_unmultiplied(
                                                                                                accent.r(),
                                                                                                accent.g(),
                                                                                                accent.b(),
                                                                                                105,
                                                                                            )
                                                                                        } else {
                                                                                            Color32::from_rgba_unmultiplied(
                                                                                                255, 255, 255, 20,
                                                                                            )
                                                                                        },
                                                                                    );
                                                                                    p.rect_stroke(
                                                                                        pr,
                                                                                        CornerRadius::same(8),
                                                                                        if focused {
                                                                                            Stroke::new(1.2_f32, Color32::WHITE)
                                                                                        } else {
                                                                                            Stroke::new(
                                                                                                1.0_f32,
                                                                                                Color32::from_rgba_unmultiplied(
                                                                                                    255, 255, 255, 28,
                                                                                                ),
                                                                                            )
                                                                                        },
                                                                                        egui::StrokeKind::Inside,
                                                                                    );
                                                                                    p.text(
                                                                                        pr.center(),
                                                                                        Align2::CENTER_CENTER,
                                                                                        &row.val,
                                                                                        FontId::proportional(11.5),
                                                                                        Color32::WHITE,
                                                                                    );
                                                                                }
                                                                                _ => {
                                                                                    let ch_c = Pos2::new(
                                                                                        rect.right() - 20.0,
                                                                                        rect.center().y,
                                                                                    );
                                                                                    p.circle_filled(
                                                                                        ch_c,
                                                                                        9.5,
                                                                                        if focused {
                                                                                            Color32::from_rgba_unmultiplied(
                                                                                                accent.r(),
                                                                                                accent.g(),
                                                                                                accent.b(),
                                                                                                135,
                                                                                            )
                                                                                        } else {
                                                                                            Color32::from_rgba_unmultiplied(
                                                                                                accent.r(),
                                                                                                accent.g(),
                                                                                                accent.b(),
                                                                                                48,
                                                                                            )
                                                                                        },
                                                                                    );
                                                                                    p.add(egui::Shape::line(
                                                                                        vec![
                                                                                            ch_c + Vec2::new(-2.0, -3.5),
                                                                                            ch_c + Vec2::new(2.2, 0.0),
                                                                                            ch_c + Vec2::new(-2.0, 3.5),
                                                                                        ],
                                                                                        Stroke::new(
                                                                                            1.8_f32,
                                                                                            if focused { Color32::WHITE } else { accent },
                                                                                        ),
                                                                                    ));
                                                                                    p.text(
                                                                                        Pos2::new(
                                                                                            rect.right() - 37.0,
                                                                                            rect.center().y,
                                                                                        ),
                                                                                        Align2::RIGHT_CENTER,
                                                                                        &row.val,
                                                                                        FontId::proportional(11.5),
                                                                                        if focused {
                                                                                            Color32::WHITE
                                                                                        } else if row.is_on {
                                                                                            Color32::from_rgb(110, 231, 183)
                                                                                        } else {
                                                                                            Color32::from_rgb(176, 192, 218)
                                                                                        },
                                                                                    );
                                                                                }
                                                                            }

                                                                            if focused
                                                                                && self
                                                                                    .modal_focus_changed
                                                                            {
                                                                                resp.scroll_to_me(
                                                                                    Some(
                                                                                        Align::Center,
                                                                                    ),
                                                                                );
                                                                                self.modal_focus_changed =
                                                                                    false;
                                                                            }
                                                                            if resp.clicked() {
                                                                                self.settings_zone =
                                                                                    SettingsZone::List;
                                                                                self.modal_focus =
                                                                                    idx;
                                                                                activate_list =
                                                                                    Some((
                                                                                        tab, idx,
                                                                                    ));
                                                                            }
                                                                            ui.add_space(5.0);
                                                                        }
                                                                    });
                                                            },
                                                        );
                                                    });
                                            },
                                        );

                                        // RIGHT 40%: Live Preview & Quick Tools Stage (explicit top-down!)
                                        let right_fill = Color32::from_rgba_unmultiplied(
                                            fill.r().saturating_sub(4),
                                            fill.g().saturating_sub(4),
                                            fill.b().saturating_sub(2),
                                            fill.a().saturating_add(16),
                                        );
                                        ui.allocate_ui_with_layout(
                                            Vec2::new(right_w, body_h),
                                            egui::Layout::top_down(Align::LEFT),
                                            |ui| {
                                                egui::Frame::NONE
                                                    .fill(right_fill)
                                                    .stroke(Stroke::new(
                                                        1.2_f32,
                                                        Color32::from_rgba_unmultiplied(
                                                            255, 255, 255, 42,
                                                        ),
                                                    ))
                                                    .corner_radius(CornerRadius::same(18))
                                                    .inner_margin(egui::Margin::same(14))
                                                    .show(ui, |ui| {
                                                        let inner_w = right_w - 28.0;
                                                        let inner_h = body_h - 28.0;
                                                        ui.set_min_size(Vec2::new(
                                                            inner_w, inner_h,
                                                        ));
                                                        ui.allocate_ui_with_layout(
                                                            Vec2::new(inner_w, inner_h),
                                                            egui::Layout::top_down(Align::LEFT),
                                                            |ui| {
                                                                ui.set_min_height(inner_h);
                                                                ui.spacing_mut().item_spacing =
                                                                    Vec2::new(6.0, 0.0);

                                                                let cur = &rows[self
                                                                    .modal_focus
                                                                    .min(rows.len() - 1)];

                                                                // Header bar: LIVE PREVIEW + contextual hint
                                                                ui.horizontal(|ui| {
                                                                    ui.label(
                                                                        RichText::new(
                                                                            "LIVE PREVIEW",
                                                                        )
                                                                        .font(FontId::proportional(
                                                                            10.5,
                                                                        ))
                                                                        .strong()
                                                                        .color(accent),
                                                                    );
                                                                    ui.with_layout(
                                                                        egui::Layout::right_to_left(
                                                                            Align::Center,
                                                                        ),
                                                                        |ui| {
                                                                            let hint = match cur.kind
                                                                            {
                                                                                0 => "OK Toggle",
                                                                                1 => "OK Cycle",
                                                                                _ => "OK Open",
                                                                            };
                                                                            ui.label(
                                                                                RichText::new(hint)
                                                                                    .font(
                                                                                        FontId::proportional(
                                                                                            10.0,
                                                                                        ),
                                                                                    )
                                                                                    .color(
                                                                                        Color32::from_rgb(
                                                                                            162,
                                                                                            180,
                                                                                            208,
                                                                                        ),
                                                                                    ),
                                                                            );
                                                                        },
                                                                    );
                                                                });
                                                                ui.add_space(6.0);

                                                                // Dynamic Clipped Vector Preview Canvas
                                                                let (preview_rect, _) = ui
                                                                    .allocate_exact_size(
                                                                        Vec2::new(inner_w, 162.0),
                                                                        Sense::hover(),
                                                                    );
                                                                let clip_p =
                                                                    ui.painter_at(preview_rect);
                                                                self.draw_settings_live_preview(
                                                                    ctx,
                                                                    &clip_p,
                                                                    preview_rect,
                                                                    tab,
                                                                    self.modal_focus,
                                                                    visible.len(),
                                                                );

                                                                ui.add_space(10.0);

                                                                // Context Info for Focused Item (expands cleanly so Quick Tools align flush with bottom of left card)
                                                                let info_h = (inner_h - 286.0)
                                                                    .max(66.0);
                                                                ui.allocate_ui_with_layout(
                                                                    Vec2::new(inner_w, info_h),
                                                                    egui::Layout::top_down(
                                                                        Align::LEFT,
                                                                    ),
                                                                    |ui| {
                                                                        ui.set_min_height(info_h);
                                                                        ui.horizontal(|ui| {
                                                                            let (ir, _) = ui
                                                                                .allocate_exact_size(
                                                                                    Vec2::splat(
                                                                                        22.0,
                                                                                    ),
                                                                                    Sense::hover(),
                                                                                );
                                                                            let sc = Self::setting_icon_color(
                                                                                cur.icon_id,
                                                                            );
                                                                            ui.painter()
                                                                                .circle_filled(
                                                                                    ir.center(),
                                                                                    11.0,
                                                                                    Color32::from_rgba_unmultiplied(
                                                                                        sc.r(),
                                                                                        sc.g(),
                                                                                        sc.b(),
                                                                                        36,
                                                                                    ),
                                                                                );
                                                                            Self::draw_setting_icon(
                                                                                ui.painter(),
                                                                                ir.center(),
                                                                                cur.icon_id,
                                                                                sc,
                                                                            );
                                                                            ui.label(
                                                                                RichText::new(
                                                                                    cur.label,
                                                                                )
                                                                                .font(
                                                                                    FontId::proportional(
                                                                                        14.5,
                                                                                    ),
                                                                                )
                                                                                .strong()
                                                                                .color(
                                                                                    Color32::WHITE,
                                                                                ),
                                                                            );
                                                                        });
                                                                        ui.add_space(4.0);
                                                                        ui.label(
                                                                            RichText::new(cur.desc)
                                                                                .font(
                                                                                    FontId::proportional(
                                                                                        11.5,
                                                                                    ),
                                                                                )
                                                                                .color(
                                                                                    Color32::from_rgb(
                                                                                        188, 204,
                                                                                        228,
                                                                                    ),
                                                                                ),
                                                                        );
                                                                    },
                                                                );

                                                                // Bottom Pinned Android TV Shortcuts & Boost RAM QuickBar
                                                                let (sep_r, _) = ui
                                                                    .allocate_exact_size(
                                                                        Vec2::new(inner_w, 1.0),
                                                                        Sense::hover(),
                                                                    );
                                                                ui.painter().rect_filled(
                                                                    sep_r,
                                                                    CornerRadius::ZERO,
                                                                    Color32::from_rgba_unmultiplied(
                                                                        255, 255, 255, 24,
                                                                    ),
                                                                );
                                                                ui.add_space(6.0);
                                                                ui.label(
                                                                    RichText::new(
                                                                        "ANDROID TV & QUICK TOOLS",
                                                                    )
                                                                    .font(FontId::proportional(9.5))
                                                                    .strong()
                                                                    .color(Color32::from_rgb(
                                                                        148, 166, 196,
                                                                    )),
                                                                );
                                                                ui.add_space(5.0);

                                                                let shortcuts = [
                                                                    ("Network", 200usize),
                                                                    ("Apps", 201usize),
                                                                    ("Display", 202usize),
                                                                    ("System", 204usize),
                                                                ];
                                                                let btn_gap = 6.0_f32;
                                                                let btn_w =
                                                                    (inner_w - btn_gap * 3.0) / 4.0;
                                                                ui.horizontal(|ui| {
                                                                    ui.spacing_mut()
                                                                        .item_spacing
                                                                        .x = btn_gap;
                                                                    for (c, (lbl, ic)) in
                                                                        shortcuts.iter().enumerate()
                                                                    {
                                                                        let f = self.settings_zone
                                                                            == SettingsZone::QuickBar
                                                                            && self
                                                                                .settings_sys_col
                                                                                == c;
                                                                        let (br, bresp) = ui
                                                                            .allocate_exact_size(
                                                                                Vec2::new(
                                                                                    btn_w, 29.0,
                                                                                ),
                                                                                Sense::click(),
                                                                            );
                                                                        let bp = ui.painter();
                                                                        bp.rect_filled(
                                                                            br,
                                                                            CornerRadius::same(8),
                                                                            if f {
                                                                                Color32::from_rgba_unmultiplied(
                                                                                    accent.r(),
                                                                                    accent.g(),
                                                                                    accent.b(),
                                                                                    98,
                                                                                )
                                                                            } else {
                                                                                Color32::from_rgba_unmultiplied(
                                                                                    255, 255, 255,
                                                                                    15,
                                                                                )
                                                                            },
                                                                        );
                                                                        bp.rect_stroke(
                                                                            br,
                                                                            CornerRadius::same(8),
                                                                            if f {
                                                                                Stroke::new(
                                                                                    1.8_f32,
                                                                                    Color32::WHITE,
                                                                                )
                                                                            } else {
                                                                                Stroke::new(
                                                                                    1.0_f32,
                                                                                    Color32::from_rgba_unmultiplied(
                                                                                        255, 255,
                                                                                        255, 24,
                                                                                    ),
                                                                                )
                                                                            },
                                                                            egui::StrokeKind::Inside,
                                                                        );
                                                                        Self::draw_setting_icon(
                                                                            bp,
                                                                            Pos2::new(
                                                                                br.left() + 14.0,
                                                                                br.center().y,
                                                                            ),
                                                                            *ic,
                                                                            Color32::WHITE,
                                                                        );
                                                                        bp.text(
                                                                            Pos2::new(
                                                                                br.left() + 26.0,
                                                                                br.center().y,
                                                                            ),
                                                                            Align2::LEFT_CENTER,
                                                                            *lbl,
                                                                            FontId::proportional(
                                                                                10.5,
                                                                            ),
                                                                            Color32::WHITE,
                                                                        );
                                                                        if bresp.clicked() {
                                                                            self.settings_zone =
                                                                                SettingsZone::QuickBar;
                                                                            self.settings_sys_col =
                                                                                c;
                                                                            activate_quick =
                                                                                Some(c);
                                                                        }
                                                                    }
                                                                });

                                                                ui.add_space(6.0);

                                                                // Full-width Boost RAM Pill (QuickBar index 4)
                                                                let boost_f = self.settings_zone
                                                                    == SettingsZone::QuickBar
                                                                    && self.settings_sys_col == 4;
                                                                let (rr, rresp) = ui
                                                                    .allocate_exact_size(
                                                                        Vec2::new(inner_w, 30.0),
                                                                        Sense::click(),
                                                                    );
                                                                let rp = ui.painter();
                                                                rp.rect_filled(
                                                                    rr,
                                                                    CornerRadius::same(9),
                                                                    if boost_f {
                                                                        Color32::from_rgba_unmultiplied(
                                                                            accent.r(),
                                                                            accent.g(),
                                                                            accent.b(),
                                                                            110,
                                                                        )
                                                                    } else {
                                                                        Color32::from_rgba_unmultiplied(
                                                                            accent.r(),
                                                                            accent.g(),
                                                                            accent.b(),
                                                                            32,
                                                                        )
                                                                    },
                                                                );
                                                                rp.rect_stroke(
                                                                    rr,
                                                                    CornerRadius::same(9),
                                                                    if boost_f {
                                                                        Stroke::new(
                                                                            1.8_f32,
                                                                            Color32::WHITE,
                                                                        )
                                                                    } else {
                                                                        Stroke::new(
                                                                            1.0_f32,
                                                                            Color32::from_rgba_unmultiplied(
                                                                                accent.r(),
                                                                                accent.g(),
                                                                                accent.b(),
                                                                                110,
                                                                            ),
                                                                        )
                                                                    },
                                                                    egui::StrokeKind::Inside,
                                                                );
                                                                Self::draw_setting_icon(
                                                                    rp,
                                                                    Pos2::new(
                                                                        rr.left() + 18.0,
                                                                        rr.center().y,
                                                                    ),
                                                                    101,
                                                                    Color32::WHITE,
                                                                );
                                                                rp.text(
                                                                    Pos2::new(
                                                                        rr.left() + 34.0,
                                                                        rr.center().y,
                                                                    ),
                                                                    Align2::LEFT_CENTER,
                                                                    format!(
                                                                        "Boost RAM  ·  {} / {} MB free",
                                                                        self.status.ram_free_mb,
                                                                        self.status.ram_total_mb
                                                                    ),
                                                                    FontId::proportional(11.5),
                                                                    Color32::WHITE,
                                                                );
                                                                if rresp.clicked() {
                                                                    self.settings_zone =
                                                                        SettingsZone::QuickBar;
                                                                    self.settings_sys_col = 4;
                                                                    activate_quick = Some(4);
                                                                }
                                                            },
                                                        );
                                                    });
                                            },
                                        );
                                    },
                                );
                            },
                        );
                    });

                // Apply mouse tab / close clicks
                if clicked_close {
                    self.close_modal_step();
                    return;
                }
                if let Some(t_idx) = clicked_tab {
                    self.settings_tab = t_idx;
                    self.settings_ribbon_focus = t_idx;
                    self.settings_zone = SettingsZone::List;
                    self.modal_focus = 0;
                    self.modal_focus_changed = true;
                }

                // Execute QuickBar action
                if let Some(q_idx) = activate_quick {
                    match q_idx {
                        0 => bridge::open_system_settings("network"),
                        1 => bridge::open_system_settings("apps"),
                        2 => bridge::open_system_settings("display"),
                        3 => bridge::open_system_settings("main"),
                        _ => self.spawn_clear_ram(ctx.clone()),
                    }
                }

                // Execute Selected Setting Action
                if let Some((t_idx, r_idx)) = activate_list {
                    match (t_idx, r_idx) {
                        // Tab 0: Appearance
                        (0, 0) => self.open_modal(ActiveModal::Theme),
                        (0, 1) => {
                            self.config.layout_style = match self.config.layout_style {
                                LayoutStyle::Zenith => LayoutStyle::Classic,
                                LayoutStyle::Classic => LayoutStyle::Zenith,
                            };
                            self.save_config();
                            self.show_toast(format!(
                                "Switched to {}",
                                self.config.layout_style.label()
                            ));
                        }
                        (0, 2) => {
                            self.config.screensaver_style = self.config.screensaver_style.next();
                            self.save_config();
                            self.show_toast(format!(
                                "Screensaver Style: {}",
                                self.config.screensaver_style.label()
                            ));
                        }
                        (0, 3) => {
                            self.config.ambient_timeout_secs =
                                match self.config.ambient_timeout_secs {
                                    0 => 60,
                                    60 => 180,
                                    180 => 300,
                                    _ => 0,
                                };
                            self.save_config();
                        }
                        (0, 4) => {
                            self.config.icon_size = match self.config.icon_size {
                                IconSize::Compact => IconSize::Standard,
                                IconSize::Standard => IconSize::Large,
                                IconSize::Large => IconSize::Compact,
                            };
                            self.save_config();
                        }
                        (0, 5) => {
                            self.config.show_header = !self.config.show_header;
                            self.save_config();
                        }
                        (0, 6) => {
                            self.config.show_labels = !self.config.show_labels;
                            self.save_config();
                        }
                        (0, 7) => {
                            self.config.auto_night_dim = !self.config.auto_night_dim;
                            self.save_config();
                        }
                        (0, 8) => {
                            self.config.lightweight_mode = !self.config.lightweight_mode;
                            let tol = if self.config.lightweight_mode { 0.6 } else { 0.2 };
                            ctx.options_mut(|o| {
                                o.tessellation_options.bezier_tolerance = tol;
                            });
                            self.wallpaper_tex = None;
                            self.wallpaper_loading = None;
                            self.save_config();
                            self.ensure_wallpaper(ctx.clone());
                            self.show_toast(if self.config.lightweight_mode {
                                "Lightweight Mode ON (low CPU/VRAM)"
                            } else {
                                "Full 4K Quality Mode ON"
                            });
                        }
                        // Tab 1: Weather & HUD
                        (1, 0) => {
                            self.config.show_weather = !self.config.show_weather;
                            self.save_config();
                        }
                        (1, 1) => self.open_modal(ActiveModal::WeatherCity),
                        (1, 2) => {
                            self.config.clock_24h = !self.config.clock_24h;
                            self.cached_sec = u32::MAX;
                            self.save_config();
                        }
                        (1, 3) => {
                            self.config.show_status_pill = !self.config.show_status_pill;
                            self.save_config();
                        }
                        (1, 4) => {
                            self.config.show_now_playing = !self.config.show_now_playing;
                            self.save_config();
                        }
                        (1, 5) => self.open_modal(ActiveModal::Overlay),
                        // Tab 2: Gemini AI
                        (2, 0) => {
                            self.config.gemini_enabled = !self.config.gemini_enabled;
                            if !self.config.gemini_enabled {
                                self.gemini_reply.clear();
                                self.gemini_last_id = None;
                            }
                            self.save_config();
                            self.show_toast(if self.config.gemini_enabled {
                                "Gemini AI ON"
                            } else {
                                "Gemini AI OFF"
                            });
                        }
                        (2, 1) => {
                            self.config.gemini_model =
                                gemini::next_model(&self.config.gemini_model).to_string();
                            self.gemini_last_id = None;
                            self.save_config();
                            self.show_toast(format!("Gemini Model: {}", self.config.gemini_model));
                        }
                        (2, 2) => {
                            self.gemini_text_mode = false;
                            self.open_modal(ActiveModal::Gemini);
                            if !self.config.gemini_api_key.trim().is_empty() && !self.gemini_busy {
                                self.start_gemini_voice_listen();
                            }
                        }
                        (2, 3) => self.open_modal(ActiveModal::GeminiSettings),
                        (2, 4) => {
                            let p = "Recommend 3 great movies or shows to watch tonight with a quick spoken pitch for each.".to_string();
                            self.gemini_prompt = p.clone();
                            self.gemini_text_mode = false;
                            self.open_modal(ActiveModal::Gemini);
                            self.spawn_ask_gemini(ctx.clone(), p);
                        }
                        (2, 5) => {
                            self.a2ui.clear_custom();
                            self.gemini_reply.clear();
                            self.gemini_last_id = None;
                            self.a2ui_feedback =
                                "Cleared Gemini chat & custom surfaces".to_string();
                            self.show_toast("Cleared Gemini chat & custom surfaces");
                        }
                        // Tab 3: Apps
                        (3, 0) => {
                            self.config.sort_order = match self.config.sort_order {
                                SortOrder::Alpha => SortOrder::Recent,
                                SortOrder::Recent => SortOrder::Alpha,
                            };
                            self.save_config();
                        }
                        (3, 1) => {
                            self.active_folder_idx = None;
                            self.open_modal(ActiveModal::Folders);
                        }
                        (3, 2) => {
                            self.config.show_mobile_apps = !self.config.show_mobile_apps;
                            self.save_config();
                        }
                        (3, 3) => {
                            self.config.show_system_apps = !self.config.show_system_apps;
                            self.save_config();
                        }
                        (3, 4) => {
                            self.config.auto_clear_ram_on_home =
                                !self.config.auto_clear_ram_on_home;
                            self.save_config();
                        }
                        (3, 5) => {
                            self.config.hidden_apps.clear();
                            self.save_config();
                            self.show_toast("Restored all hidden apps");
                        }
                        // Tab 4: System
                        (4, 0) => self.open_modal(ActiveModal::StartupOptions),
                        (4, 1) => {
                            let ok = bridge::open_home_settings();
                            if !ok {
                                self.show_toast("Open Android Settings → Apps → Default Home");
                            }
                        }
                        (4, 2) => self.open_modal(ActiveModal::PowerMenu),
                        (4, 3) => {
                            self.local_ip = bridge::get_local_ip();
                            self.open_modal(ActiveModal::PhoneCompanion);
                        }
                        (4, 4) => self.open_modal(ActiveModal::Backup),
                        (4, 5) => {
                            self.open_modal(ActiveModal::Updater);
                            if matches!(
                                self.updater_state,
                                UpdaterState::Idle | UpdaterState::Error(_)
                            ) {
                                self.spawn_check_update(ctx.clone(), false);
                            }
                        }
                        (4, 6) => self.open_modal(ActiveModal::Donate),
                        _ => {}
                    }
                }
            }

            ActiveModal::Search => {
                let q = self.search_query.to_lowercase();
                let matches: Vec<TvAppInfo> = visible
                    .iter()
                    .filter(|a| {
                        q.is_empty()
                            || a.label.to_lowercase().contains(&q)
                            || a.package_name.to_lowercase().contains(&q)
                    })
                    .cloned()
                    .collect();

                let mut launch_pkg: Option<String> = None;
                if allow_keys {
                    ctx.input(|i| {
                        if i.key_pressed(Key::ArrowDown) && !matches.is_empty() {
                            self.modal_focus = (self.modal_focus + 1).min(matches.len() - 1);
                        } else if i.key_pressed(Key::ArrowUp) {
                            self.modal_focus = self.modal_focus.saturating_sub(1);
                        } else if i.key_pressed(Key::Enter) {
                            if let Some(a) = matches.get(self.modal_focus) {
                                launch_pkg = Some(a.package_name.clone());
                            }
                        }
                    });
                }
                if let Some(pkg) = launch_pkg {
                    self.modal = ActiveModal::None;
                    self.launch_app(&pkg);
                    return;
                }

                self.draw_modal_shell(ctx, "Search Apps", 680.0, 420.0, |this, ui, ctx| {
                    let accent = this.config.accent();
                    ui.horizontal(|ui| {
                        ui.label("Filter:");
                        ui.text_edit_singleline(&mut this.search_query);
                        if Self::modal_btn(ui, "Clear", false, false, accent) {
                            this.search_query.clear();
                            this.modal_focus = 0;
                        }
                    });
                    ui.add_space(10.0);

                    let full_w = ui.available_width();
                    for (idx, app) in matches.iter().take(24).enumerate() {
                        this.request_icon(&app.package_name, ctx);
                        let focused = this.modal_focus == idx;
                        let fill = if focused {
                            Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 85)
                        } else {
                            Color32::from_rgba_unmultiplied(255, 255, 255, 14)
                        };
                        let stroke = if focused {
                            Stroke::new(2.0_f32, accent)
                        } else {
                            Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 24))
                        };
                        let (rect, resp) = ui.allocate_exact_size(Vec2::new(full_w, 38.0), Sense::click());
                        let p = ui.painter();
                        p.rect_filled(rect, CornerRadius::same(10), fill);
                        p.rect_stroke(rect, CornerRadius::same(10), stroke, egui::StrokeKind::Inside);

                        let ic_c = Pos2::new(rect.left() + 22.0, rect.center().y);
                        if let Some(tex) = this.icon_textures.get(&app.package_name) {
                            p.image(
                                tex.id(),
                                Rect::from_center_size(ic_c, Vec2::splat(26.0)),
                                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                                Color32::WHITE,
                            );
                        } else {
                            p.circle_filled(ic_c, 13.0, Color32::from_rgb(40, 50, 74));
                        }
                        let disp = this.display_label(app);
                        p.text(
                            Pos2::new(rect.left() + 44.0, rect.center().y),
                            Align2::LEFT_CENTER,
                            format!("{disp}   ·   {}", app.package_name),
                            FontId::proportional(13.5),
                            Color32::WHITE,
                        );
                        if resp.clicked() {
                            let pkg = app.package_name.clone();
                            this.modal = ActiveModal::None;
                            this.launch_app(&pkg);
                        }
                        ui.add_space(5.0);
                    }
                });
            }

            ActiveModal::Gemini => {
                // Pure floating Voice Orb overlay — handle BACK / Escape / Backspace directly
                // since this overlay intentionally renders without draw_modal_shell().
                let jni_back = bridge::take_back_pressed();
                let key_back = ctx.input(|i| i.key_pressed(Key::Escape) || i.key_pressed(Key::Backspace));
                if (jni_back || key_back) && self.last_back_handled.elapsed() > Duration::from_millis(260) {
                    self.last_back_handled = Instant::now();
                    self.close_modal_step();
                    return;
                }

                // Hands-free silence VAD auto-submits when user pauses speaking.
                if self.gemini_listening && bridge::is_voice_auto_stopped() {
                    self.finish_gemini_voice_listen(ctx.clone());
                } else if self.gemini_listening && !bridge::is_voice_recording() {
                    self.gemini_listening = false;
                }

                let is_speaking = bridge::is_speaking();
                let audio_level = if self.gemini_listening {
                    bridge::get_voice_level()
                } else if is_speaking {
                    bridge::get_speak_level()
                } else {
                    0.0
                };

                let mut toggle_voice = false;
                if allow_keys {
                    ctx.input(|i| {
                        if i.key_pressed(Key::Enter) || i.key_pressed(Key::Space) {
                            toggle_voice = true;
                        }
                    });
                }

                let screen = ctx.screen_rect();
                let accent = self.config.accent();
                let listening_now = self.gemini_listening;
                let busy_now = self.gemini_busy;
                let has_key = !self.config.gemini_api_key.trim().is_empty();

                // Soft full-screen vignette scrim behind the floating orb
                egui::Area::new(egui::Id::new("gemini_orb_scrim"))
                    .fixed_pos(screen.min)
                    .order(egui::Order::Middle)
                    .show(ctx, |ui| {
                        let (r, resp) = ui.allocate_exact_size(screen.size(), Sense::click());
                        ui.painter().rect_filled(
                            r,
                            CornerRadius::ZERO,
                            Color32::from_rgba_unmultiplied(4, 7, 16, 178),
                        );
                        if resp.clicked() {
                            self.close_modal_step();
                        }
                    });

                // Floating centered Voice Orb ONLY + minimal caption/subtitle
                egui::Area::new(egui::Id::new("gemini_orb_overlay"))
                    .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                    .order(egui::Order::Foreground)
                    .show(ctx, |ui| {
                        ui.set_width(580.0);
                        ui.vertical_centered(|ui| {
                            let orb_resp = self.draw_gemini_voice_orb(
                                ui,
                                ctx,
                                listening_now,
                                busy_now,
                                is_speaking,
                                audio_level,
                            );
                            if orb_resp.clicked() {
                                toggle_voice = true;
                            }

                            ui.add_space(6.0);
                            let caption = if !has_key {
                                "Set Gemini API key in Settings → Gemini AI"
                            } else if listening_now {
                                "Listening..."
                            } else if busy_now {
                                "Thinking..."
                            } else if is_speaking {
                                "Speaking..."
                            } else {
                                "Press OK to Speak"
                            };
                            ui.label(
                                RichText::new(caption)
                                    .font(FontId::proportional(17.0))
                                    .strong()
                                    .color(if listening_now {
                                        Color32::from_rgb(52, 211, 153)
                                    } else if !has_key {
                                        Color32::from_rgb(251, 191, 36)
                                    } else {
                                        accent
                                    }),
                            );

                            if !self.gemini_reply.is_empty() {
                                ui.add_space(12.0);
                                ui.label(
                                    RichText::new(&self.gemini_reply)
                                        .font(FontId::proportional(16.0))
                                        .color(Color32::from_rgba_unmultiplied(240, 246, 255, 235)),
                                );
                            } else if !self.a2ui_feedback.is_empty()
                                && self.a2ui_feedback.starts_with("Gemini error:")
                            {
                                ui.add_space(8.0);
                                ui.label(
                                    RichText::new(&self.a2ui_feedback)
                                        .font(FontId::proportional(13.5))
                                        .color(Color32::from_rgb(248, 113, 113)),
                                );
                            }
                        });
                    });

                if toggle_voice {
                    if !has_key {
                        self.open_modal(ActiveModal::GeminiSettings);
                    } else if self.gemini_listening {
                        self.finish_gemini_voice_listen(ctx.clone());
                    } else if is_speaking {
                        bridge::stop_speaking();
                        self.start_gemini_voice_listen();
                    } else if !self.gemini_busy {
                        self.start_gemini_voice_listen();
                    }
                }
            }

            ActiveModal::GeminiSettings => {
                const ITEM_COUNT: usize = 7;
                let mut trigger = false;
                if allow_keys {
                    ctx.input(|i| {
                        if i.key_pressed(Key::ArrowDown) {
                            self.modal_focus = (self.modal_focus + 1).min(ITEM_COUNT - 1);
                            self.modal_focus_changed = true;
                        } else if i.key_pressed(Key::ArrowUp) {
                            if self.modal_focus == 0 {
                                self.modal_close_focused = true;
                            } else {
                                self.modal_focus -= 1;
                                self.modal_focus_changed = true;
                            }
                        } else if i.key_pressed(Key::Enter) || i.key_pressed(Key::Space) {
                            trigger = true;
                        }
                    });
                }

                let mut chosen: Option<usize> = if trigger {
                    Some(self.modal_focus)
                } else {
                    None
                };

                let has_key = !self.config.gemini_api_key.trim().is_empty();
                let companion_url = format!("http://{}:{WEB_COMPANION_PORT}", self.local_ip);

                self.draw_modal_shell(
                    ctx,
                    "Gemini AI Settings  (Up/Down move · OK select)",
                    760.0,
                    470.0,
                    |this, ui, _ctx| {
                        let accent = this.config.accent();
                        let rows: [(&str, String, bool); ITEM_COUNT] = [
                            (
                                "Gemini AI & Top-Bar Icon",
                                if this.config.gemini_enabled { "ON" } else { "OFF" }.to_string(),
                                this.config.gemini_enabled,
                            ),
                            (
                                "Gemini Model",
                                this.config.gemini_model.clone(),
                                true,
                            ),
                            (
                                "API Key (via Phone Companion)",
                                if has_key {
                                    "✓ Configured ›".to_string()
                                } else {
                                    format!("Set at {companion_url} ›")
                                },
                                has_key,
                            ),
                            (
                                "Send Text Prompt",
                                if this.gemini_busy { "Thinking..." } else { "Send" }.to_string(),
                                false,
                            ),
                            (
                                "Quick Voice: What to Watch",
                                "Run".to_string(),
                                false,
                            ),
                            (
                                "Quick Voice: Evening Briefing",
                                "Run".to_string(),
                                false,
                            ),
                            (
                                "Clear Chat & A2UI Surfaces",
                                "Reset".to_string(),
                                false,
                            ),
                        ];

                        for (idx, (label, val, is_on)) in rows.iter().enumerate() {
                            let focused = this.modal_focus == idx;
                            egui::Frame::NONE
                                .fill(if focused {
                                    Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 38)
                                } else {
                                    Color32::from_rgba_unmultiplied(255, 255, 255, 10)
                                })
                                .stroke(if focused {
                                    Stroke::new(1.8_f32, accent)
                                } else {
                                    Stroke::new(
                                        1.0_f32,
                                        Color32::from_rgba_unmultiplied(255, 255, 255, 20),
                                    )
                                })
                                .corner_radius(CornerRadius::same(11))
                                .inner_margin(egui::Margin::symmetric(14, 9))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            RichText::new(*label)
                                                .font(FontId::proportional(14.0))
                                                .strong()
                                                .color(Color32::WHITE),
                                        );
                                        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                                            let pill_fill = if focused {
                                                accent
                                            } else if *is_on {
                                                Color32::from_rgba_unmultiplied(52, 211, 153, 38)
                                            } else {
                                                Color32::from_rgba_unmultiplied(255, 255, 255, 16)
                                            };
                                            if ui
                                                .add(
                                                    egui::Button::new(
                                                        RichText::new(val)
                                                            .font(FontId::proportional(12.5))
                                                            .strong()
                                                            .color(Color32::WHITE),
                                                    )
                                                    .fill(pill_fill)
                                                    .corner_radius(CornerRadius::same(8)),
                                                )
                                                .clicked()
                                            {
                                                this.modal_focus = idx;
                                                chosen = Some(idx);
                                            }
                                        });
                                    });
                                });
                            ui.add_space(5.0);
                        }

                        ui.add_space(4.0);
                        ui.label(
                            RichText::new("TEXT PROMPT")
                                .font(FontId::proportional(10.5))
                                .strong()
                                .color(Color32::from_rgb(160, 178, 204)),
                        );
                        ui.add(
                            egui::TextEdit::singleline(&mut this.gemini_prompt)
                                .desired_width(f32::INFINITY)
                                .hint_text("Type a prompt for Send Text Prompt above..."),
                        );
                        if !this.a2ui_feedback.is_empty() {
                            ui.add_space(4.0);
                            ui.label(
                                RichText::new(&this.a2ui_feedback)
                                    .font(FontId::proportional(12.5))
                                    .color(accent)
                                    .strong(),
                            );
                        }
                    },
                );

                if let Some(act) = chosen {
                    match act {
                        0 => {
                            self.config.gemini_enabled = !self.config.gemini_enabled;
                            if !self.config.gemini_enabled {
                                self.gemini_reply.clear();
                                self.gemini_last_id = None;
                            }
                            self.save_config();
                            self.show_toast(if self.config.gemini_enabled {
                                "Gemini AI ON"
                            } else {
                                "Gemini AI OFF"
                            });
                        }
                        1 => {
                            self.config.gemini_model =
                                gemini::next_model(&self.config.gemini_model).to_string();
                            self.gemini_last_id = None;
                            self.save_config();
                            self.show_toast(format!("Gemini Model: {}", self.config.gemini_model));
                        }
                        2 => {
                            self.local_ip = bridge::get_local_ip();
                            self.open_modal(ActiveModal::PhoneCompanion);
                        }
                        3 => {
                            let p = self.gemini_prompt.trim().to_string();
                            if !p.is_empty() {
                                self.gemini_text_mode = false;
                                self.open_modal(ActiveModal::Gemini);
                                self.spawn_ask_gemini(ctx.clone(), p);
                            }
                        }
                        4 => {
                            let p = "Recommend 3 great movies or shows to watch tonight with a quick spoken pitch for each.".to_string();
                            self.gemini_prompt = p.clone();
                            self.gemini_text_mode = false;
                            self.open_modal(ActiveModal::Gemini);
                            self.spawn_ask_gemini(ctx.clone(), p);
                        }
                        5 => {
                            let p = "Give me a warm, concise evening TV briefing based on my current time, city, and weather.".to_string();
                            self.gemini_prompt = p.clone();
                            self.gemini_text_mode = false;
                            self.open_modal(ActiveModal::Gemini);
                            self.spawn_ask_gemini(ctx.clone(), p);
                        }
                        _ => {
                            self.a2ui.clear_custom();
                            self.gemini_reply.clear();
                            self.gemini_last_id = None;
                            self.a2ui_feedback = "Cleared Gemini chat & custom surfaces".to_string();
                            self.show_toast("Cleared Gemini chat & custom surfaces");
                        }
                    }
                }
            }

            ActiveModal::A2ui => {
                let mut trigger_btn = false;
                if allow_keys {
                    ctx.input(|i| {
                        if i.key_pressed(Key::ArrowLeft) {
                            self.modal_focus = self.modal_focus.saturating_sub(1);
                        } else if i.key_pressed(Key::ArrowRight) {
                            self.modal_focus = (self.modal_focus + 1).min(4);
                        } else if i.key_pressed(Key::Enter) || i.key_pressed(Key::Space) {
                            trigger_btn = true;
                        }
                    });
                }
                if trigger_btn {
                    match self.modal_focus {
                        0 => {
                            let p = self.gemini_prompt.trim().to_string();
                            if !p.is_empty() {
                                self.spawn_ask_gemini(ctx.clone(), p);
                            }
                        }
                        1 => {
                            let p = self.gemini_prompt.trim().to_string();
                            let prompt = if p.is_empty() {
                                "Create a Tonight on TV dashboard shelf".to_string()
                            } else {
                                p
                            };
                            self.spawn_gemini_a2ui(ctx.clone(), prompt);
                        }
                        2 => {
                            self.config.gemini_model =
                                gemini::next_model(&self.config.gemini_model).to_string();
                            self.gemini_last_id = None;
                            self.save_config();
                            self.a2ui_feedback =
                                format!("Switched Gemini model to {}", self.config.gemini_model);
                        }
                        3 => match self.a2ui.process_json(&self.a2ui_input) {
                            Ok(n) => {
                                self.a2ui_feedback =
                                    format!("✓ Applied {n} A2UI v1.0 message(s) natively in Rust");
                            }
                            Err(e) => {
                                self.a2ui_feedback = format!("Error: {e}");
                            }
                        },
                        _ => {
                            self.a2ui.clear_custom();
                            self.gemini_reply.clear();
                            self.gemini_last_id = None;
                            self.a2ui_feedback = "Cleared Gemini chat & custom surfaces".to_string();
                        }
                    }
                }

                let mut ask_clicked = false;
                let mut a2ui_clicked = false;
                let mut model_clicked = false;
                let model_lbl = format!("Model: {}", self.config.gemini_model);
                let gemini_on = self.config.gemini_enabled;
                let has_key = !self.config.gemini_api_key.trim().is_empty();
                let companion_url = format!("http://{}:{WEB_COMPANION_PORT}", self.local_ip);

                self.draw_modal_shell(
                    ctx,
                    if gemini_on {
                        "✨ Gemini AI & A2UI Studio  (Left/Right select · OK run)"
                    } else {
                        "A2UI v1.0 Native Surface Studio  (Gemini AI disabled in Settings)"
                    },
                    800.0,
                    480.0,
                    |this, ui, _ctx| {
                        let accent = this.config.accent();
                        if !gemini_on {
                            ui.label(
                                RichText::new("Gemini AI is disabled — enable it in Settings → Gemini AI")
                                    .color(Color32::from_rgb(251, 191, 36))
                                    .strong(),
                            );
                            ui.add_space(4.0);
                        } else if !has_key {
                            ui.label(
                                RichText::new(format!(
                                    "Add your Gemini API key via Phone Companion ({companion_url})"
                                ))
                                .color(Color32::from_rgb(251, 191, 36))
                                .strong(),
                            );
                            ui.add_space(4.0);
                        }

                        ui.horizontal_wrapped(|ui| {
                            let ask_lbl = if this.gemini_busy {
                                "✨ Thinking..."
                            } else {
                                "✨ Ask Gemini"
                            };
                            if Self::modal_btn(ui, ask_lbl, true, this.modal_focus == 0, accent) {
                                this.modal_focus = 0;
                                ask_clicked = true;
                            }
                            if Self::modal_btn(ui, "🧩 Build A2UI Shelf", false, this.modal_focus == 1, accent) {
                                this.modal_focus = 1;
                                a2ui_clicked = true;
                            }
                            if Self::modal_btn(ui, &model_lbl, false, this.modal_focus == 2, accent) {
                                this.modal_focus = 2;
                                model_clicked = true;
                            }
                            if Self::modal_btn(ui, "Inject JSON", false, this.modal_focus == 3, accent) {
                                this.modal_focus = 3;
                                match this.a2ui.process_json(&this.a2ui_input) {
                                    Ok(n) => {
                                        this.a2ui_feedback =
                                            format!("✓ Applied {n} A2UI v1.0 message(s) natively in Rust");
                                    }
                                    Err(e) => {
                                        this.a2ui_feedback = format!("Error: {e}");
                                    }
                                }
                            }
                            if Self::modal_btn(ui, "Clear", false, this.modal_focus == 4, accent) {
                                this.modal_focus = 4;
                                this.a2ui.clear_custom();
                                this.gemini_reply.clear();
                                this.gemini_last_id = None;
                                this.a2ui_feedback = "Cleared Gemini chat & custom surfaces".to_string();
                            }
                        });

                        if !this.a2ui_feedback.is_empty() {
                            ui.add_space(6.0);
                            ui.label(RichText::new(&this.a2ui_feedback).color(accent).strong());
                        }

                        ui.add_space(6.0);
                        ui.label(
                            RichText::new("GEMINI PROMPT")
                                .font(FontId::proportional(11.0))
                                .strong()
                                .color(Color32::from_rgb(160, 178, 204)),
                        );
                        ui.add(
                            egui::TextEdit::singleline(&mut this.gemini_prompt)
                                .desired_width(f32::INFINITY)
                                .hint_text("Ask Gemini anything or describe a TV shelf..."),
                        );

                        if !this.gemini_reply.is_empty() {
                            ui.add_space(8.0);
                            egui::Frame::NONE
                                .fill(Color32::from_rgba_unmultiplied(15, 23, 42, 195))
                                .stroke(Stroke::new(1.0_f32, accent))
                                .corner_radius(CornerRadius::same(12))
                                .inner_margin(egui::Margin::same(12))
                                .show(ui, |ui| {
                                    ScrollArea::vertical()
                                        .max_height(150.0)
                                        .show(ui, |ui| {
                                            ui.label(
                                                RichText::new(&this.gemini_reply)
                                                    .font(FontId::proportional(14.5))
                                                    .color(Color32::WHITE),
                                            );
                                        });
                                });
                        }

                        ui.add_space(8.0);
                        ui.label(
                            RichText::new("A2UI v1.0 PAYLOAD")
                                .font(FontId::proportional(11.0))
                                .strong()
                                .color(Color32::from_rgb(160, 178, 204)),
                        );
                        ui.add(
                            egui::TextEdit::multiline(&mut this.a2ui_input)
                                .desired_rows(5)
                                .desired_width(f32::INFINITY)
                                .code_editor(),
                        );
                    },
                );

                if ask_clicked {
                    let p = self.gemini_prompt.trim().to_string();
                    if !p.is_empty() {
                        self.spawn_ask_gemini(ctx.clone(), p);
                    }
                }
                if a2ui_clicked {
                    let p = self.gemini_prompt.trim().to_string();
                    let prompt = if p.is_empty() {
                        "Create a Tonight on TV dashboard shelf".to_string()
                    } else {
                        p
                    };
                    self.spawn_gemini_a2ui(ctx.clone(), prompt);
                }
                if model_clicked {
                    self.config.gemini_model =
                        gemini::next_model(&self.config.gemini_model).to_string();
                    self.gemini_last_id = None;
                    self.save_config();
                    self.a2ui_feedback =
                        format!("Switched Gemini model to {}", self.config.gemini_model);
                }
            }

            ActiveModal::AppContext(pkg) => {
                let app_label = self
                    .apps
                    .iter()
                    .find(|a| a.package_name == pkg)
                    .map(|a| self.display_label(a))
                    .unwrap_or_else(|| pkg.clone());
                let is_fav = self.config.favorites.contains(&pkg);
                let cur_folder = self
                    .config
                    .app_folders
                    .iter()
                    .find(|f| f.packages.contains(&pkg))
                    .map(|f| f.name.as_str())
                    .unwrap_or("None");
                let folder_lbl = format!("Folder: {cur_folder}  (OK cycle)");

                let actions: Vec<(&str, String)> = vec![
                    ("open", "Open App".to_string()),
                    (
                        "pin",
                        if is_fav {
                            "Unpin from Favorites"
                        } else {
                            "Pin to Favorites"
                        }
                        .to_string(),
                    ),
                    ("folder", folder_lbl),
                    ("left", "Move App Left".to_string()),
                    ("right", "Move App Right".to_string()),
                    ("rename", "Rename App".to_string()),
                    ("hide", "Hide App".to_string()),
                    ("info", "Android App Info".to_string()),
                    ("uninstall", "Uninstall App".to_string()),
                ];

                let mut chosen: Option<&str> = None;
                if allow_keys {
                    ctx.input(|i| {
                        if i.key_pressed(Key::ArrowDown) {
                            self.modal_focus = (self.modal_focus + 1).min(actions.len() - 1);
                        } else if i.key_pressed(Key::ArrowUp) {
                            self.modal_focus = self.modal_focus.saturating_sub(1);
                        } else if i.key_pressed(Key::Enter) || i.key_pressed(Key::Space) {
                            chosen = Some(actions[self.modal_focus].0);
                        }
                    });
                }

                self.draw_modal_shell(
                    ctx,
                    &format!("App Options · {app_label}"),
                    430.0,
                    495.0,
                    |this, ui, _ctx| {
                        let accent = this.config.accent();
                        ui.label(
                            RichText::new(&pkg)
                                .font(FontId::proportional(11.5))
                                .color(Color32::from_rgb(175, 192, 218)),
                        );
                        ui.add_space(8.0);

                        egui::ScrollArea::vertical()
                            .id_salt("app_options_scroll")
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                let full_w = ui.available_width();
                                for (idx, (id, label)) in actions.iter().enumerate() {
                                    let focused = this.modal_focus == idx;
                                    let is_uninstall = *id == "uninstall";
                                    let fill = if focused {
                                        if is_uninstall {
                                            Color32::from_rgba_unmultiplied(239, 68, 68, 95)
                                        } else {
                                            Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 85)
                                        }
                                    } else if is_uninstall {
                                        Color32::from_rgba_unmultiplied(239, 68, 68, 22)
                                    } else {
                                        Color32::from_rgba_unmultiplied(255, 255, 255, 16)
                                    };
                                    let stroke = if focused {
                                        if is_uninstall {
                                            Stroke::new(2.0_f32, Color32::from_rgb(248, 113, 113))
                                        } else {
                                            Stroke::new(2.0_f32, accent)
                                        }
                                    } else if is_uninstall {
                                        Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(248, 113, 113, 70))
                                    } else {
                                        Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 26))
                                    };
                                    let (rect, resp) = ui.allocate_exact_size(Vec2::new(full_w, 34.0), Sense::click());
                                    if focused {
                                        resp.scroll_to_me(Some(egui::Align::Center));
                                    }
                                    let p = ui.painter();
                                    p.rect_filled(rect, CornerRadius::same(10), fill);
                                    p.rect_stroke(rect, CornerRadius::same(10), stroke, egui::StrokeKind::Inside);

                                    let ic_c = Pos2::new(rect.left() + 22.0, rect.center().y);
                                    let ic_col = if is_uninstall {
                                        Color32::from_rgb(248, 113, 113)
                                    } else if *id == "pin" {
                                        Color32::from_rgb(250, 204, 21)
                                    } else {
                                        Color32::WHITE
                                    };
                                    Self::draw_action_icon(p, ic_c, id, ic_col);
                                    let txt_col = if is_uninstall {
                                        Color32::from_rgb(254, 202, 202)
                                    } else {
                                        Color32::WHITE
                                    };
                                    p.text(
                                        Pos2::new(rect.left() + 42.0, rect.center().y),
                                        Align2::LEFT_CENTER,
                                        label.as_str(),
                                        FontId::proportional(14.0),
                                        txt_col,
                                    );
                                    if resp.clicked() {
                                        chosen = Some(*id);
                                    }
                                    ui.add_space(5.0);
                                }
                            });
                    },
                );

                if let Some(act) = chosen {
                    match act {
                        "open" => {
                            self.modal = ActiveModal::None;
                            self.launch_app(&pkg);
                        }
                        "pin" => {
                            self.toggle_favorite(&pkg);
                            self.modal = ActiveModal::None;
                        }
                        "folder" => {
                            const PRESETS: [&str; 4] = ["Streaming", "Live TV", "Games", "Tools"];
                            let cur_idx = self
                                .config
                                .app_folders
                                .iter()
                                .find(|f| f.packages.contains(&pkg))
                                .and_then(|f| PRESETS.iter().position(|n| *n == f.name));
                            for f in &mut self.config.app_folders {
                                f.packages.retain(|p| p != &pkg);
                            }
                            let next_name = match cur_idx {
                                None => Some(PRESETS[0]),
                                Some(i) if i + 1 < PRESETS.len() => Some(PRESETS[i + 1]),
                                _ => None,
                            };
                            if let Some(name) = next_name {
                                if let Some(f) =
                                    self.config.app_folders.iter_mut().find(|f| f.name == name)
                                {
                                    f.packages.push(pkg.clone());
                                } else {
                                    self.config.app_folders.push(AppFolder {
                                        name: name.to_string(),
                                        packages: vec![pkg.clone()],
                                    });
                                }
                                self.show_toast(format!("Added to {name} folder"));
                            } else {
                                self.show_toast("Removed from folder");
                            }
                            self.config.app_folders.retain(|f| !f.packages.is_empty());
                            self.save_config();
                        }
                        "left" => {
                            self.move_app_order(&pkg, -1);
                            self.show_toast("Moved left");
                        }
                        "right" => {
                            self.move_app_order(&pkg, 1);
                            self.show_toast("Moved right");
                        }
                        "rename" => {
                            self.rename_input = app_label;
                            self.open_modal(ActiveModal::RenameApp(pkg));
                        }
                        "hide" => {
                            self.hide_app(&pkg);
                            self.modal = ActiveModal::None;
                        }
                        "info" => {
                            bridge::open_app_info(&pkg);
                            self.modal = ActiveModal::None;
                        }
                        "uninstall" => {
                            self.pending_apps_refresh = true;
                            bridge::uninstall_app(&pkg);
                            self.modal = ActiveModal::None;
                        }
                        _ => {}
                    }
                }
            }

            ActiveModal::RenameApp(pkg) => {
                let mut action: Option<u8> = None;
                if allow_keys {
                    ctx.input(|i| {
                        if i.key_pressed(Key::ArrowLeft) {
                            self.modal_focus = self.modal_focus.saturating_sub(1);
                        } else if i.key_pressed(Key::ArrowRight) {
                            self.modal_focus = (self.modal_focus + 1).min(1);
                        } else if i.key_pressed(Key::Enter) {
                            action = Some(self.modal_focus as u8);
                        }
                    });
                }

                self.draw_modal_shell(ctx, "Rename App", 460.0, 220.0, |this, ui, _ctx| {
                    let accent = this.config.accent();
                    ui.label(
                        RichText::new("Custom display name (leave blank to reset):")
                            .font(FontId::proportional(13.0))
                            .color(Color32::from_rgb(205, 220, 240)),
                    );
                    ui.add_space(8.0);
                    ui.add(
                        egui::TextEdit::singleline(&mut this.rename_input)
                            .desired_width(f32::INFINITY),
                    );
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if Self::modal_btn(ui, "Save Name", true, this.modal_focus == 0, accent) {
                            action = Some(0);
                        }
                        if Self::modal_btn(ui, "Reset Default", false, this.modal_focus == 1, accent) {
                            action = Some(1);
                        }
                    });
                });

                if let Some(a) = action {
                    if a == 0 && !self.rename_input.trim().is_empty() {
                        self.config
                            .custom_labels
                            .insert(pkg, self.rename_input.trim().to_string());
                        self.save_config();
                        self.show_toast("Saved custom app name");
                    } else {
                        self.config.custom_labels.remove(&pkg);
                        self.save_config();
                        self.show_toast("Reset app name to default");
                    }
                    self.close_modal_step();
                }
            }

            ActiveModal::Updater => {
                let mut action: Option<u8> = None;
                let is_busy = matches!(
                    self.updater_state,
                    UpdaterState::Checking | UpdaterState::Downloading { .. }
                );
                if allow_keys {
                    ctx.input(|i| {
                        if i.key_pressed(Key::ArrowLeft) {
                            self.modal_focus = self.modal_focus.saturating_sub(1);
                        } else if i.key_pressed(Key::ArrowRight) {
                            self.modal_focus = (self.modal_focus + 1).min(1);
                        } else if i.key_pressed(Key::ArrowUp) {
                            self.modal_close_focused = true;
                        } else if i.key_pressed(Key::Enter) || i.key_pressed(Key::Space) {
                            action = Some(self.modal_focus as u8);
                        }
                    });
                }

                if is_busy {
                    ctx.request_repaint();
                }

                let state_snapshot = self.updater_state.clone();
                let current_ver = self.app_version.clone();

                self.draw_modal_shell(
                    ctx,
                    "Internal Updater  ·  GitHub Releases",
                    680.0,
                    405.0,
                    |this, ui, ctx| {
                        let accent = this.config.accent();
                        let t = this.start_time.elapsed().as_secs_f32();

                        // Header card: installed version + GitHub repo
                        egui::Frame::NONE
                            .fill(Color32::from_rgba_unmultiplied(255, 255, 255, 12))
                            .stroke(Stroke::new(
                                1.0_f32,
                                Color32::from_rgba_unmultiplied(255, 255, 255, 24),
                            ))
                            .corner_radius(CornerRadius::same(14))
                            .inner_margin(egui::Margin::symmetric(14, 10))
                            .show(ui, |ui| {
                                ui.set_min_width(ui.available_width());
                                ui.horizontal(|ui| {
                                    let (ir, _) =
                                        ui.allocate_exact_size(Vec2::splat(30.0), Sense::hover());
                                    ui.painter().circle_filled(
                                        ir.center(),
                                        15.0,
                                        Color32::from_rgba_unmultiplied(
                                            accent.r(),
                                            accent.g(),
                                            accent.b(),
                                            46,
                                        ),
                                    );
                                    Self::draw_setting_icon(ui.painter(), ir.center(), 13, accent);
                                    ui.add_space(6.0);
                                    ui.vertical(|ui| {
                                        ui.label(
                                            RichText::new("Astryx Rust Launcher")
                                                .font(FontId::proportional(15.0))
                                                .strong()
                                                .color(Color32::WHITE),
                                        );
                                        ui.label(
                                            RichText::new(format!(
                                                "Installed v{}  ·  github.com/{}",
                                                current_ver,
                                                updater::GITHUB_REPO
                                            ))
                                            .font(FontId::proportional(11.5))
                                            .color(Color32::from_rgb(175, 192, 218)),
                                        );
                                    });
                                });
                            });

                        ui.add_space(10.0);

                        // Main status panel
                        egui::Frame::NONE
                            .fill(Color32::from_rgba_unmultiplied(10, 14, 24, 155))
                            .stroke(Stroke::new(
                                1.0_f32,
                                Color32::from_rgba_unmultiplied(255, 255, 255, 22),
                            ))
                            .corner_radius(CornerRadius::same(14))
                            .inner_margin(egui::Margin::symmetric(16, 12))
                            .show(ui, |ui| {
                                ui.set_min_width(ui.available_width());
                                ui.set_min_height(130.0);

                                match &state_snapshot {
                                    UpdaterState::Idle | UpdaterState::Checking => {
                                        ui.add_space(18.0);
                                        ui.vertical_centered(|ui| {
                                            let (sr, _) = ui.allocate_exact_size(
                                                Vec2::splat(34.0),
                                                Sense::hover(),
                                            );
                                            let p = ui.painter();
                                            let c = sr.center();
                                            p.circle_stroke(
                                                c,
                                                13.0,
                                                Stroke::new(
                                                    2.5_f32,
                                                    Color32::from_rgba_unmultiplied(
                                                        255, 255, 255, 28,
                                                    ),
                                                ),
                                            );
                                            let start_a = t * 5.2;
                                            let mut arc = Vec::with_capacity(14);
                                            for k in 0..=12 {
                                                let a = start_a + (k as f32 / 12.0) * 2.4;
                                                arc.push(c + Vec2::new(a.cos() * 13.0, a.sin() * 13.0));
                                            }
                                            p.add(egui::Shape::line(
                                                arc,
                                                Stroke::new(2.8_f32, accent),
                                            ));
                                            ui.add_space(10.0);
                                            ui.label(
                                                RichText::new("Checking GitHub Releases...")
                                                    .font(FontId::proportional(14.5))
                                                    .strong()
                                                    .color(Color32::WHITE),
                                            );
                                        });
                                    }

                                    UpdaterState::UpToDate { latest } => {
                                        let ok_c = Color32::from_rgb(52, 211, 153);
                                        ui.add_space(12.0);
                                        ui.vertical_centered(|ui| {
                                            let (cr, _) = ui.allocate_exact_size(
                                                Vec2::splat(36.0),
                                                Sense::hover(),
                                            );
                                            let p = ui.painter();
                                            let c = cr.center();
                                            p.circle_filled(
                                                c,
                                                17.0,
                                                Color32::from_rgba_unmultiplied(52, 211, 153, 40),
                                            );
                                            p.circle_stroke(c, 17.0, Stroke::new(1.8_f32, ok_c));
                                            p.add(egui::Shape::line(
                                                vec![
                                                    c + Vec2::new(-6.5, 0.5),
                                                    c + Vec2::new(-1.8, 5.0),
                                                    c + Vec2::new(7.0, -4.8),
                                                ],
                                                Stroke::new(2.5_f32, ok_c),
                                            ));
                                            ui.add_space(8.0);
                                            ui.label(
                                                RichText::new("You're up to date!")
                                                    .font(FontId::proportional(16.0))
                                                    .strong()
                                                    .color(Color32::WHITE),
                                            );
                                            ui.add_space(4.0);
                                            let detail = if let Some(l) = latest {
                                                format!(
                                                    "Installed v{current_ver} matches latest release v{l}."
                                                )
                                            } else {
                                                format!(
                                                    "Running v{current_ver}. Publish a GitHub Release with an .apk to push OTA updates."
                                                )
                                            };
                                            ui.label(
                                                RichText::new(detail)
                                                    .font(FontId::proportional(12.0))
                                                    .color(Color32::from_rgb(185, 202, 226)),
                                            );
                                        });
                                    }

                                    UpdaterState::Available(rel) => {
                                        ui.horizontal(|ui| {
                                            egui::Frame::NONE
                                                .fill(Color32::from_rgba_unmultiplied(
                                                    accent.r(),
                                                    accent.g(),
                                                    accent.b(),
                                                    65,
                                                ))
                                                .corner_radius(CornerRadius::same(8))
                                                .inner_margin(egui::Margin::symmetric(10, 4))
                                                .show(ui, |ui| {
                                                    ui.label(
                                                        RichText::new(format!(
                                                            "NEW  v{} → v{}",
                                                            current_ver, rel.version
                                                        ))
                                                        .font(FontId::proportional(12.0))
                                                        .strong()
                                                        .color(Color32::WHITE),
                                                    );
                                                });
                                            ui.add_space(6.0);
                                            ui.label(
                                                RichText::new(format!(
                                                    "{}  ·  {}",
                                                    updater::format_mb(rel.apk_size),
                                                    if rel.published_date.is_empty() {
                                                        "Latest Release"
                                                    } else {
                                                        rel.published_date.as_str()
                                                    }
                                                ))
                                                .font(FontId::proportional(12.0))
                                                .color(Color32::from_rgb(180, 198, 224)),
                                            );
                                        });
                                        ui.add_space(6.0);
                                        ui.label(
                                            RichText::new(&rel.title)
                                                .font(FontId::proportional(14.5))
                                                .strong()
                                                .color(Color32::WHITE),
                                        );
                                        ui.add_space(4.0);
                                        ScrollArea::vertical()
                                            .id_salt("updater_notes")
                                            .max_height(84.0)
                                            .show(ui, |ui| {
                                                ui.label(
                                                    RichText::new(&rel.notes)
                                                        .font(FontId::proportional(12.0))
                                                        .color(Color32::from_rgb(205, 218, 238)),
                                                );
                                            });
                                    }

                                    UpdaterState::Downloading {
                                        release,
                                        downloaded,
                                        total,
                                    } => {
                                        let ratio = if *total > 0 {
                                            (*downloaded as f32 / *total as f32).clamp(0.0, 1.0)
                                        } else {
                                            0.0
                                        };
                                        let pct = (ratio * 100.0).round() as u32;
                                        ui.add_space(16.0);
                                        ui.label(
                                            RichText::new(format!(
                                                "Downloading v{} ({})...",
                                                release.version, release.apk_name
                                            ))
                                            .font(FontId::proportional(14.0))
                                            .strong()
                                            .color(Color32::WHITE),
                                        );
                                        ui.add_space(10.0);
                                        let (bar_r, _) = ui.allocate_exact_size(
                                            Vec2::new(ui.available_width(), 14.0),
                                            Sense::hover(),
                                        );
                                        let p = ui.painter();
                                        p.rect_filled(
                                            bar_r,
                                            CornerRadius::same(7),
                                            Color32::from_rgba_unmultiplied(255, 255, 255, 24),
                                        );
                                        let mut fill_r = bar_r;
                                        fill_r.set_width((bar_r.width() * ratio).max(10.0));
                                        p.rect_filled(fill_r, CornerRadius::same(7), accent);
                                        ui.add_space(8.0);
                                        ui.horizontal(|ui| {
                                            ui.label(
                                                RichText::new(format!(
                                                    "{} / {}",
                                                    updater::format_mb(*downloaded),
                                                    updater::format_mb(*total)
                                                ))
                                                .font(FontId::proportional(12.5))
                                                .color(Color32::from_rgb(190, 206, 230)),
                                            );
                                            ui.with_layout(
                                                egui::Layout::right_to_left(Align::Center),
                                                |ui| {
                                                    ui.label(
                                                        RichText::new(format!("{pct}%"))
                                                            .font(FontId::proportional(13.5))
                                                            .strong()
                                                            .color(Color32::WHITE),
                                                    );
                                                },
                                            );
                                        });
                                        ctx.request_repaint();
                                    }

                                    UpdaterState::ReadyToInstall { release, .. } => {
                                        ui.add_space(16.0);
                                        ui.vertical_centered(|ui| {
                                            ui.label(
                                                RichText::new(format!(
                                                    "✓ v{} Downloaded & Verified",
                                                    release.version
                                                ))
                                                .font(FontId::proportional(15.5))
                                                .strong()
                                                .color(Color32::from_rgb(52, 211, 153)),
                                            );
                                            ui.add_space(6.0);
                                            ui.label(
                                                RichText::new(
                                                    "Confirm the Android TV system prompt to finish updating, or press 'Install Update' below.",
                                                )
                                                .font(FontId::proportional(12.0))
                                                .color(Color32::from_rgb(200, 214, 236)),
                                            );
                                        });
                                    }

                                    UpdaterState::NeedPermission { release, .. } => {
                                        ui.add_space(14.0);
                                        ui.vertical_centered(|ui| {
                                            ui.label(
                                                RichText::new(format!(
                                                    "Permission Needed to Install v{}",
                                                    release.version
                                                ))
                                                .font(FontId::proportional(15.0))
                                                .strong()
                                                .color(Color32::from_rgb(251, 191, 36)),
                                            );
                                            ui.add_space(6.0);
                                            ui.label(
                                                RichText::new(
                                                    "Enable 'Install unknown apps' for Astryx Rust Launcher in Android TV Settings, then press 'Install Update'.",
                                                )
                                                .font(FontId::proportional(12.0))
                                                .color(Color32::from_rgb(210, 222, 240)),
                                            );
                                        });
                                    }

                                    UpdaterState::Error(err) => {
                                        ui.add_space(16.0);
                                        ui.vertical_centered(|ui| {
                                            ui.label(
                                                RichText::new("Update Notice")
                                                    .font(FontId::proportional(15.0))
                                                    .strong()
                                                    .color(Color32::from_rgb(248, 113, 113)),
                                            );
                                            ui.add_space(6.0);
                                            ui.label(
                                                RichText::new(err)
                                                    .font(FontId::proportional(12.0))
                                                    .color(Color32::from_rgb(215, 225, 242)),
                                            );
                                        });
                                    }
                                }
                            });

                        ui.add_space(10.0);

                        let primary_lbl = match &state_snapshot {
                            UpdaterState::Available(_) => "Download & Install",
                            UpdaterState::Downloading { .. } => "Downloading...",
                            UpdaterState::ReadyToInstall { .. }
                            | UpdaterState::NeedPermission { .. } => "Install Update",
                            UpdaterState::Checking => "Checking...",
                            _ => "Check for Updates",
                        };
                        let secondary_lbl = match &state_snapshot {
                            UpdaterState::Available(_)
                            | UpdaterState::ReadyToInstall { .. }
                            | UpdaterState::NeedPermission { .. } => "Check Again",
                            _ => "Close",
                        };

                        ui.horizontal(|ui| {
                            if Self::modal_btn(
                                ui,
                                primary_lbl,
                                true,
                                this.modal_focus == 0,
                                accent,
                            ) {
                                action = Some(0);
                            }
                            ui.add_space(8.0);
                            if Self::modal_btn(
                                ui,
                                secondary_lbl,
                                false,
                                this.modal_focus == 1,
                                accent,
                            ) {
                                action = Some(1);
                            }
                        });
                    },
                );

                if let Some(a) = action {
                    match a {
                        0 => match self.updater_state.clone() {
                            UpdaterState::Available(rel) => {
                                self.spawn_download_update(ctx.clone(), rel);
                            }
                            UpdaterState::ReadyToInstall { release, apk_path }
                            | UpdaterState::NeedPermission { release, apk_path } => {
                                self.trigger_install_apk(release, apk_path);
                            }
                            UpdaterState::Checking | UpdaterState::Downloading { .. } => {}
                            _ => {
                                self.spawn_check_update(ctx.clone(), false);
                            }
                        },
                        1 => match self.updater_state {
                            UpdaterState::Available(_)
                            | UpdaterState::ReadyToInstall { .. }
                            | UpdaterState::NeedPermission { .. } => {
                                self.spawn_check_update(ctx.clone(), false);
                            }
                            _ => {
                                self.close_modal_step();
                            }
                        },
                        _ => {}
                    }
                }
            }

            ActiveModal::Overlay => {
                const ROW_COUNT: usize = 7;
                let mut trigger = false;
                if allow_keys {
                    ctx.input(|i| {
                        if i.key_pressed(Key::ArrowDown) {
                            self.modal_focus = (self.modal_focus + 1).min(ROW_COUNT - 1);
                            self.modal_focus_changed = true;
                        } else if i.key_pressed(Key::ArrowUp) {
                            if self.modal_focus == 0 {
                                self.modal_close_focused = true;
                            } else {
                                self.modal_focus -= 1;
                                self.modal_focus_changed = true;
                            }
                        } else if i.key_pressed(Key::Enter)
                            || i.key_pressed(Key::Space)
                            || i.key_pressed(Key::ArrowLeft)
                            || i.key_pressed(Key::ArrowRight)
                        {
                            trigger = true;
                        }
                    });
                }

                let apply_overlay_row = |this: &mut Self, idx: usize| {
                    match idx {
                        0 => this.config.show_fps_meter = !this.config.show_fps_meter,
                        1 => this.config.overlay_show_fps = !this.config.overlay_show_fps,
                        2 => this.config.overlay_show_hz = !this.config.overlay_show_hz,
                        3 => this.config.overlay_show_cpu = !this.config.overlay_show_cpu,
                        4 => this.config.overlay_show_gpu = !this.config.overlay_show_gpu,
                        5 => this.config.overlay_show_net = !this.config.overlay_show_net,
                        6 => this.config.overlay_show_ram = !this.config.overlay_show_ram,
                        _ => {}
                    }
                    this.fps_frame_count = 0;
                    this.fps_window_start = Instant::now();
                    this.save_config();
                    let res =
                        bridge::set_fps_overlay(this.config.show_fps_meter, this.config.overlay_flags());
                    this.fps_overlay_active = res == "ON";
                    if idx == 0 {
                        if this.config.show_fps_meter {
                            if res == "ON" {
                                this.show_toast("HUD Overlay ON (visible across all apps)");
                            } else if res == "NEED_PERMISSION" {
                                this.show_toast(
                                    "Allow 'Display over other apps' for cross-app HUD overlay",
                                );
                            }
                        } else {
                            this.show_toast("HUD Overlay OFF");
                        }
                    }
                };

                if trigger {
                    apply_overlay_row(self, self.modal_focus);
                }

                self.draw_modal_shell(
                    ctx,
                    "HUD Overlay  ·  Up/Down move · OK toggle · BACK Settings",
                    660.0,
                    440.0,
                    |this, ui, _ctx| {
                        let accent = this.config.accent();

                        // Live Pill Preview
                        let mut preview_parts: Vec<String> = Vec::new();
                        if this.config.overlay_show_fps {
                            preview_parts.push(format!("{} FPS", this.fps_current.round() as u32));
                        }
                        if this.config.overlay_show_hz {
                            preview_parts.push(format!("{:.0}Hz", this.display_hz));
                        }
                        if this.config.overlay_show_cpu {
                            preview_parts.push("CPU 34%".to_string());
                        }
                        if this.config.overlay_show_gpu {
                            preview_parts.push("GPU 18%".to_string());
                        }
                        if this.config.overlay_show_net {
                            preview_parts.push("↓ 8.4 MB/s".to_string());
                        }
                        if this.config.overlay_show_ram {
                            preview_parts.push(format!("RAM {} MB", this.status.ram_free_mb));
                        }
                        let preview_txt = if !this.config.show_fps_meter {
                            "Overlay Disabled".to_string()
                        } else if preview_parts.is_empty() {
                            "All measurements hidden".to_string()
                        } else {
                            preview_parts.join("  ·  ")
                        };

                        ui.vertical_centered(|ui| {
                            egui::Frame::NONE
                                .fill(Color32::from_rgba_unmultiplied(12, 16, 28, 224))
                                .stroke(Stroke::new(
                                    1.2_f32,
                                    if this.config.show_fps_meter {
                                        accent
                                    } else {
                                        Color32::from_rgba_unmultiplied(255, 255, 255, 36)
                                    },
                                ))
                                .corner_radius(CornerRadius::same(18))
                                .inner_margin(egui::Margin::symmetric(16, 7))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing.x = 7.0;
                                        let (dr, _) =
                                            ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
                                        ui.painter().circle_filled(
                                            dr.center(),
                                            3.6,
                                            if this.config.show_fps_meter {
                                                Color32::from_rgb(52, 211, 153)
                                            } else {
                                                Color32::from_rgb(120, 132, 154)
                                            },
                                        );
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(preview_txt)
                                                    .font(FontId::proportional(12.5))
                                                    .strong()
                                                    .color(Color32::WHITE),
                                            )
                                            .extend(),
                                        );
                                    });
                                });
                        });

                        ui.add_space(12.0);

                        let items: [(&str, &str, bool, usize); ROW_COUNT] = [
                            (
                                "Cross-App HUD Overlay",
                                "Floating top-center pill over launcher & all apps",
                                this.config.show_fps_meter,
                                14,
                            ),
                            (
                                "Live FPS",
                                "Real-time frame rate counter",
                                this.config.overlay_show_fps,
                                14,
                            ),
                            (
                                "Screen Refresh Rate",
                                "Active TV display refresh rate (Hz)",
                                this.config.overlay_show_hz,
                                100,
                            ),
                            (
                                "CPU Usage",
                                "Live processor utilization (%)",
                                this.config.overlay_show_cpu,
                                101,
                            ),
                            (
                                "GPU Usage",
                                "Live graphics load / frame pressure (%)",
                                this.config.overlay_show_gpu,
                                202,
                            ),
                            (
                                "Network Activity",
                                "Live download & upload speed (KB/s · MB/s)",
                                this.config.overlay_show_net,
                                200,
                            ),
                            (
                                "Free Memory (RAM)",
                                "Available system RAM in MB",
                                this.config.overlay_show_ram,
                                102,
                            ),
                        ];

                        let mut clicked: Option<usize> = None;
                        for (idx, (title, sub, is_on, icon_id)) in items.iter().enumerate() {
                            let focused = this.modal_focus == idx;
                            let resp = egui::Frame::NONE
                                .fill(if focused {
                                    Color32::from_rgba_unmultiplied(
                                        accent.r(),
                                        accent.g(),
                                        accent.b(),
                                        44,
                                    )
                                } else {
                                    Color32::from_rgba_unmultiplied(255, 255, 255, 10)
                                })
                                .stroke(if focused {
                                    Stroke::new(1.8_f32, accent)
                                } else {
                                    Stroke::new(
                                        1.0_f32,
                                        Color32::from_rgba_unmultiplied(255, 255, 255, 18),
                                    )
                                })
                                .corner_radius(CornerRadius::same(12))
                                .inner_margin(egui::Margin::symmetric(14, 7))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        let (ir, _) = ui.allocate_exact_size(
                                            Vec2::splat(26.0),
                                            Sense::hover(),
                                        );
                                        let sc = Self::setting_icon_color(*icon_id);
                                        ui.painter().circle_filled(
                                            ir.center(),
                                            12.5,
                                            if focused {
                                                Color32::from_rgba_unmultiplied(255, 255, 255, 32)
                                            } else {
                                                Color32::from_rgba_unmultiplied(255, 255, 255, 16)
                                            },
                                        );
                                        Self::draw_setting_icon(ui.painter(), ir.center(), *icon_id, sc);
                                        ui.add_space(6.0);
                                        ui.vertical(|ui| {
                                            ui.spacing_mut().item_spacing.y = 0.0;
                                            ui.label(
                                                RichText::new(*title)
                                                    .font(FontId::proportional(13.5))
                                                    .strong()
                                                    .color(Color32::WHITE),
                                            );
                                            ui.label(
                                                RichText::new(*sub)
                                                    .font(FontId::proportional(10.5))
                                                    .color(Color32::from_rgb(172, 188, 214)),
                                            );
                                        });
                                        ui.with_layout(
                                            egui::Layout::right_to_left(Align::Center),
                                            |ui| {
                                                egui::Frame::NONE
                                                    .fill(if *is_on {
                                                        Color32::from_rgba_unmultiplied(
                                                            52, 211, 153, 55,
                                                        )
                                                    } else {
                                                        Color32::from_rgba_unmultiplied(
                                                            255, 255, 255, 16,
                                                        )
                                                    })
                                                    .stroke(Stroke::new(
                                                        1.0_f32,
                                                        if *is_on {
                                                            Color32::from_rgb(52, 211, 153)
                                                        } else {
                                                            Color32::from_rgba_unmultiplied(
                                                                255, 255, 255, 36,
                                                            )
                                                        },
                                                    ))
                                                    .corner_radius(CornerRadius::same(8))
                                                    .inner_margin(egui::Margin::symmetric(12, 4))
                                                    .show(ui, |ui| {
                                                        ui.label(
                                                            RichText::new(if *is_on {
                                                                "ON"
                                                            } else {
                                                                "OFF"
                                                            })
                                                            .font(FontId::proportional(11.5))
                                                            .strong()
                                                            .color(Color32::WHITE),
                                                        );
                                                    });
                                            },
                                        );
                                    });
                                })
                                .response;
                            if focused && this.modal_focus_changed {
                                resp.scroll_to_me(Some(Align::Center));
                                this.modal_focus_changed = false;
                            }
                            if resp.clicked() {
                                this.modal_focus = idx;
                                clicked = Some(idx);
                            }
                            ui.add_space(5.0);
                        }

                        if let Some(c) = clicked {
                            apply_overlay_row(this, c);
                        }
                    },
                );
            }

            ActiveModal::Donate => {
                let mut action: Option<u8> = None;
                if allow_keys {
                    ctx.input(|i| {
                        if i.key_pressed(Key::ArrowLeft) {
                            self.modal_focus = self.modal_focus.saturating_sub(1);
                        } else if i.key_pressed(Key::ArrowRight) {
                            self.modal_focus = (self.modal_focus + 1).min(1);
                        } else if i.key_pressed(Key::ArrowUp) {
                            self.modal_close_focused = true;
                        } else if i.key_pressed(Key::Enter) || i.key_pressed(Key::Space) {
                            action = Some(self.modal_focus as u8);
                        }
                    });
                }

                let coffee_gold = Color32::from_rgb(255, 221, 0);

                self.draw_modal_shell(
                    ctx,
                    "Buy Me a Coffee  ·  Support Astryx",
                    640.0,
                    365.0,
                    |this, ui, _ctx| {
                        let accent = this.config.accent();
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 22.0;

                            // Left: Crisp 4K vector QR code card (instant phone scan!)
                            egui::Frame::NONE
                                .fill(Color32::WHITE)
                                .stroke(Stroke::new(2.0_f32, coffee_gold))
                                .corner_radius(CornerRadius::same(16))
                                .inner_margin(egui::Margin::same(12))
                                .show(ui, |ui| {
                                    let qr_px = 186.0;
                                    let (qr_rect, _) = ui.allocate_exact_size(
                                        Vec2::splat(qr_px),
                                        Sense::hover(),
                                    );
                                    let p = ui.painter();
                                    if let Ok(code) = qrcode::QrCode::new(DONATE_URL.as_bytes()) {
                                        let width = code.width();
                                        let cell = qr_px / width as f32;
                                        for y in 0..width {
                                            for x in 0..width {
                                                if code[(x, y)] == qrcode::Color::Dark {
                                                    let min = Pos2::new(
                                                        qr_rect.left() + x as f32 * cell,
                                                        qr_rect.top() + y as f32 * cell,
                                                    );
                                                    let max = Pos2::new(
                                                        min.x + cell + 0.35,
                                                        min.y + cell + 0.35,
                                                    );
                                                    p.rect_filled(
                                                        Rect::from_min_max(min, max),
                                                        0.0,
                                                        Color32::from_rgb(14, 18, 28),
                                                    );
                                                }
                                            }
                                        }
                                    }
                                });

                            // Right: Support card & D-Pad buttons
                            ui.vertical(|ui| {
                                ui.add_space(4.0);
                                ui.horizontal(|ui| {
                                    let (ir, _) = ui.allocate_exact_size(
                                        Vec2::splat(30.0),
                                        Sense::hover(),
                                    );
                                    ui.painter().circle_filled(
                                        ir.center(),
                                        15.0,
                                        Color32::from_rgba_unmultiplied(255, 221, 0, 42),
                                    );
                                    Self::draw_setting_icon(
                                        ui.painter(),
                                        ir.center(),
                                        15,
                                        coffee_gold,
                                    );
                                    ui.label(
                                        RichText::new("Support Development")
                                            .font(FontId::proportional(17.5))
                                            .strong()
                                            .color(Color32::WHITE),
                                    );
                                });
                                ui.add_space(8.0);
                                ui.label(
                                    RichText::new(
                                        "Enjoying Astryx Rust Launcher? Scan the QR code with your phone camera to support updates and new features via Buy Me a Coffee.",
                                    )
                                    .font(FontId::proportional(12.5))
                                    .color(Color32::from_rgb(200, 214, 236)),
                                );
                                ui.add_space(12.0);
                                egui::Frame::NONE
                                    .fill(Color32::from_rgba_unmultiplied(255, 221, 0, 24))
                                    .stroke(Stroke::new(
                                        1.0_f32,
                                        Color32::from_rgba_unmultiplied(255, 221, 0, 110),
                                    ))
                                    .corner_radius(CornerRadius::same(10))
                                    .inner_margin(egui::Margin::symmetric(12, 7))
                                    .show(ui, |ui| {
                                        ui.label(
                                            RichText::new("buymeacoffee.com/burbsklopek")
                                                .font(FontId::proportional(13.0))
                                                .strong()
                                                .color(coffee_gold),
                                        );
                                    });
                                ui.add_space(18.0);
                                ui.horizontal(|ui| {
                                    if Self::modal_btn(
                                        ui,
                                        "Open in Browser",
                                        true,
                                        this.modal_focus == 0,
                                        accent,
                                    ) {
                                        action = Some(0);
                                    }
                                    ui.add_space(8.0);
                                    if Self::modal_btn(
                                        ui,
                                        "Back",
                                        false,
                                        this.modal_focus == 1,
                                        accent,
                                    ) {
                                        action = Some(1);
                                    }
                                });
                            });
                        });
                    },
                );

                if let Some(a) = action {
                    match a {
                        0 => {
                            if !bridge::open_url(DONATE_URL) {
                                self.show_toast("Scan the QR code with your phone camera");
                            }
                        }
                        _ => {
                            self.close_modal_step();
                        }
                    }
                }
            }

            ActiveModal::WeatherCity => {
                let count = WEATHER_CITY_PRESETS.len();
                let mut picked: Option<usize> = None;
                if allow_keys {
                    ctx.input(|i| {
                        if i.key_pressed(Key::ArrowDown) {
                            self.modal_focus = (self.modal_focus + 1).min(count - 1);
                            self.modal_focus_changed = true;
                        } else if i.key_pressed(Key::ArrowUp) {
                            if self.modal_focus == 0 {
                                self.modal_close_focused = true;
                            } else {
                                self.modal_focus -= 1;
                                self.modal_focus_changed = true;
                            }
                        } else if i.key_pressed(Key::Enter) || i.key_pressed(Key::Space) {
                            picked = Some(self.modal_focus);
                        }
                    });
                }

                self.draw_modal_shell(
                    ctx,
                    "Weather Location  ·  OK select · Phone Companion for any city",
                    560.0,
                    410.0,
                    |this, ui, _ctx| {
                        let accent = this.config.accent();
                        ui.label(
                            RichText::new(format!(
                                "Current: {}   (Open Phone Companion to search any custom city)",
                                this.config.weather_city
                            ))
                            .font(FontId::proportional(11.5))
                            .color(Color32::from_rgb(178, 196, 222)),
                        );
                        ui.add_space(10.0);
                        for (idx, (city, _, _)) in WEATHER_CITY_PRESETS.iter().enumerate() {
                            let focused = this.modal_focus == idx;
                            let active = this.config.weather_city == *city;
                            let w = ui.available_width();
                            let (rect, resp) =
                                ui.allocate_exact_size(Vec2::new(w, 38.0), Sense::click());
                            let p = ui.painter();
                            p.rect_filled(
                                rect,
                                CornerRadius::same(10),
                                if focused {
                                    Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 65)
                                } else {
                                    Color32::from_rgba_unmultiplied(255, 255, 255, 11)
                                },
                            );
                            p.rect_stroke(
                                rect,
                                CornerRadius::same(10),
                                if focused {
                                    Stroke::new(2.0_f32, accent)
                                } else {
                                    Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 20))
                                },
                                egui::StrokeKind::Inside,
                            );
                            p.text(
                                Pos2::new(rect.left() + 16.0, rect.center().y),
                                Align2::LEFT_CENTER,
                                *city,
                                FontId::proportional(14.0),
                                Color32::WHITE,
                            );
                            if active {
                                p.text(
                                    Pos2::new(rect.right() - 16.0, rect.center().y),
                                    Align2::RIGHT_CENTER,
                                    "✓ Active",
                                    FontId::proportional(12.0),
                                    Color32::from_rgb(52, 211, 153),
                                );
                            }
                            if focused && this.modal_focus_changed {
                                resp.scroll_to_me(Some(Align::Center));
                                this.modal_focus_changed = false;
                            }
                            if resp.clicked() {
                                picked = Some(idx);
                            }
                            ui.add_space(5.0);
                        }
                    },
                );

                if let Some(idx) = picked {
                    let (name, lat, lon) = WEATHER_CITY_PRESETS[idx];
                    self.config.weather_city = name.to_string();
                    self.config.weather_lat = lat;
                    self.config.weather_lon = lon;
                    self.save_config();
                    self.spawn_load_weather(ctx.clone());
                    self.show_toast(format!("Weather set to {name}"));
                    self.close_modal_step();
                }
            }

            ActiveModal::Folders => {
                let folders = self.config.app_folders.clone();
                let mut open_folder: Option<usize> = None;
                let mut launch_pkg: Option<String> = None;

                if let Some(fi) = self.active_folder_idx {
                    if let Some(folder) = folders.get(fi) {
                        let f_apps: Vec<TvAppInfo> = folder
                            .packages
                            .iter()
                            .filter_map(|p| self.apps.iter().find(|a| &a.package_name == p).cloned())
                            .collect();
                        if allow_keys {
                            ctx.input(|i| {
                                if i.key_pressed(Key::ArrowDown) && !f_apps.is_empty() {
                                    self.modal_focus = (self.modal_focus + 1).min(f_apps.len() - 1);
                                } else if i.key_pressed(Key::ArrowUp) {
                                    if self.modal_focus == 0 {
                                        self.modal_close_focused = true;
                                    } else {
                                        self.modal_focus -= 1;
                                    }
                                } else if (i.key_pressed(Key::Enter) || i.key_pressed(Key::Space))
                                    && self.modal_focus < f_apps.len()
                                {
                                    launch_pkg = Some(f_apps[self.modal_focus].package_name.clone());
                                }
                            });
                        }
                        self.draw_modal_shell(
                            ctx,
                            &format!("Folder · {}  (OK launch · BACK close)", folder.name),
                            560.0,
                            380.0,
                            |this, ui, _ctx| {
                                let accent = this.config.accent();
                                for (idx, app) in f_apps.iter().enumerate() {
                                    let focused = this.modal_focus == idx;
                                    let w = ui.available_width();
                                    let (rect, resp) =
                                        ui.allocate_exact_size(Vec2::new(w, 44.0), Sense::click());
                                    let p = ui.painter();
                                    p.rect_filled(
                                        rect,
                                        CornerRadius::same(10),
                                        if focused {
                                            Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 65)
                                        } else {
                                            Color32::from_rgba_unmultiplied(255, 255, 255, 11)
                                        },
                                    );
                                    p.rect_stroke(
                                        rect,
                                        CornerRadius::same(10),
                                        if focused {
                                            Stroke::new(2.0_f32, accent)
                                        } else {
                                            Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 20))
                                        },
                                        egui::StrokeKind::Inside,
                                    );
                                    let ic_c = Pos2::new(rect.left() + 24.0, rect.center().y);
                                    if let Some(tex) = this.icon_textures.get(&app.package_name) {
                                        p.image(
                                            tex.id(),
                                            Rect::from_center_size(ic_c, Vec2::splat(28.0)),
                                            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                                            Color32::WHITE,
                                        );
                                    }
                                    p.text(
                                        Pos2::new(rect.left() + 48.0, rect.center().y),
                                        Align2::LEFT_CENTER,
                                        this.display_label(app),
                                        FontId::proportional(14.5),
                                        Color32::WHITE,
                                    );
                                    if resp.clicked() {
                                        launch_pkg = Some(app.package_name.clone());
                                    }
                                    ui.add_space(6.0);
                                }
                            },
                        );
                    } else {
                        self.active_folder_idx = None;
                    }
                } else {
                    if allow_keys {
                        ctx.input(|i| {
                            if i.key_pressed(Key::ArrowDown) && !folders.is_empty() {
                                self.modal_focus = (self.modal_focus + 1).min(folders.len() - 1);
                            } else if i.key_pressed(Key::ArrowUp) {
                                if self.modal_focus == 0 {
                                    self.modal_close_focused = true;
                                } else {
                                    self.modal_focus -= 1;
                                }
                            } else if (i.key_pressed(Key::Enter) || i.key_pressed(Key::Space))
                                && self.modal_focus < folders.len()
                            {
                                open_folder = Some(self.modal_focus);
                            }
                        });
                    }
                    self.draw_modal_shell(
                        ctx,
                        "App Folders  ·  Hold OK on any app to assign folders",
                        580.0,
                        360.0,
                        |this, ui, _ctx| {
                            let accent = this.config.accent();
                            if folders.is_empty() {
                                ui.add_space(40.0);
                                ui.vertical_centered(|ui| {
                                    ui.label(
                                        RichText::new("No folders created yet")
                                            .font(FontId::proportional(16.0))
                                            .strong()
                                            .color(Color32::WHITE),
                                    );
                                    ui.add_space(6.0);
                                    ui.label(
                                        RichText::new(
                                            "Hold OK on any app on Home → choose 'Folder' to group apps into Streaming, Live TV, Games, or Tools.",
                                        )
                                        .font(FontId::proportional(12.5))
                                        .color(Color32::from_rgb(185, 202, 226)),
                                    );
                                });
                            } else {
                                for (idx, f) in folders.iter().enumerate() {
                                    let focused = this.modal_focus == idx;
                                    let w = ui.available_width();
                                    let (rect, resp) =
                                        ui.allocate_exact_size(Vec2::new(w, 46.0), Sense::click());
                                    let p = ui.painter();
                                    p.rect_filled(
                                        rect,
                                        CornerRadius::same(12),
                                        if focused {
                                            Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 65)
                                        } else {
                                            Color32::from_rgba_unmultiplied(255, 255, 255, 12)
                                        },
                                    );
                                    p.rect_stroke(
                                        rect,
                                        CornerRadius::same(12),
                                        if focused {
                                            Stroke::new(2.0_f32, accent)
                                        } else {
                                            Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 22))
                                        },
                                        egui::StrokeKind::Inside,
                                    );
                                    p.text(
                                        Pos2::new(rect.left() + 16.0, rect.center().y),
                                        Align2::LEFT_CENTER,
                                        &f.name,
                                        FontId::proportional(15.0),
                                        Color32::WHITE,
                                    );
                                    p.text(
                                        Pos2::new(rect.right() - 18.0, rect.center().y),
                                        Align2::RIGHT_CENTER,
                                        format!("{} apps  ›", f.packages.len()),
                                        FontId::proportional(12.5),
                                        accent,
                                    );
                                    if resp.clicked() {
                                        open_folder = Some(idx);
                                    }
                                    ui.add_space(6.0);
                                }
                            }
                        },
                    );
                }

                if let Some(fi) = open_folder {
                    self.active_folder_idx = Some(fi);
                    self.modal_focus = 0;
                    self.modal_just_opened = true;
                }
                if let Some(pkg) = launch_pkg {
                    self.active_folder_idx = None;
                    self.modal = ActiveModal::None;
                    self.modal_stack.clear();
                    self.launch_app(&pkg);
                }
            }

            ActiveModal::PowerMenu => {
                let mut action: Option<usize> = None;
                if allow_keys {
                    ctx.input(|i| {
                        if i.key_pressed(Key::ArrowDown) {
                            self.modal_focus = (self.modal_focus + 1).min(3);
                        } else if i.key_pressed(Key::ArrowUp) {
                            if self.modal_focus == 0 {
                                self.modal_close_focused = true;
                            } else {
                                self.modal_focus -= 1;
                            }
                        } else if i.key_pressed(Key::Enter) || i.key_pressed(Key::Space) {
                            action = Some(self.modal_focus);
                        }
                    });
                }

                self.draw_modal_shell(
                    ctx,
                    "Power & Devices  ·  Bluetooth · Display · Ambient · Sleep",
                    560.0,
                    320.0,
                    |this, ui, _ctx| {
                        let accent = this.config.accent();
                        let rows = [
                            ("Bluetooth Audio & Devices", "Pair or switch headphones / soundbar", 200),
                            ("Display & HDMI Settings", "Picture mode, 4K resolution & HDR", 100),
                            ("Start Ambient Clock Now", "OLED-safe giant UK clock & live sky", 8),
                            ("TV Sleep / Daydream", "Launch Android TV system screensaver", 101),
                        ];
                        for (idx, (title, sub, ic)) in rows.iter().enumerate() {
                            let focused = this.modal_focus == idx;
                            let resp = egui::Frame::NONE
                                .fill(if focused {
                                    Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 52)
                                } else {
                                    Color32::from_rgba_unmultiplied(255, 255, 255, 11)
                                })
                                .stroke(if focused {
                                    Stroke::new(2.0_f32, accent)
                                } else {
                                    Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 20))
                                })
                                .corner_radius(CornerRadius::same(12))
                                .inner_margin(egui::Margin::symmetric(14, 9))
                                .show(ui, |ui| {
                                    ui.set_min_width(ui.available_width());
                                    ui.horizontal(|ui| {
                                        let (ir, _) = ui.allocate_exact_size(Vec2::splat(26.0), Sense::hover());
                                        let sc = Self::setting_icon_color(*ic);
                                        ui.painter().circle_filled(
                                            ir.center(),
                                            12.5,
                                            if focused {
                                                Color32::from_rgba_unmultiplied(255, 255, 255, 32)
                                            } else {
                                                Color32::from_rgba_unmultiplied(255, 255, 255, 16)
                                            },
                                        );
                                        Self::draw_setting_icon(ui.painter(), ir.center(), *ic, sc);
                                        ui.add_space(6.0);
                                        ui.vertical(|ui| {
                                            ui.label(RichText::new(*title).font(FontId::proportional(14.0)).strong().color(Color32::WHITE));
                                            ui.label(RichText::new(*sub).font(FontId::proportional(11.0)).color(Color32::from_rgb(176, 194, 220)));
                                        });
                                    });
                                })
                                .response;
                            if resp.clicked() {
                                action = Some(idx);
                            }
                            ui.add_space(6.0);
                        }
                    },
                );

                if let Some(a) = action {
                    match a {
                        0 => bridge::open_system_settings("bluetooth"),
                        1 => bridge::open_system_settings("display"),
                        2 => {
                            self.modal = ActiveModal::None;
                            self.modal_stack.clear();
                            self.ambient_active = true;
                        }
                        _ => bridge::trigger_sleep(),
                    }
                }
            }

            ActiveModal::PhoneCompanion => {
                let url = format!("http://{}:{WEB_COMPANION_PORT}", self.local_ip);
                if allow_keys {
                    ctx.input(|i| {
                        if i.key_pressed(Key::ArrowUp) {
                            self.modal_close_focused = true;
                        } else if i.key_pressed(Key::Enter) || i.key_pressed(Key::Space) {
                            self.close_modal_step();
                        }
                    });
                }

                self.draw_modal_shell(
                    ctx,
                    "Phone Wi-Fi Companion  ·  Scan with Phone",
                    640.0,
                    365.0,
                    |this, ui, _ctx| {
                        let accent = this.config.accent();
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 22.0;
                            egui::Frame::NONE
                                .fill(Color32::WHITE)
                                .stroke(Stroke::new(2.0_f32, accent))
                                .corner_radius(CornerRadius::same(16))
                                .inner_margin(egui::Margin::same(12))
                                .show(ui, |ui| {
                                    let qr_px = 186.0;
                                    let (qr_rect, _) = ui.allocate_exact_size(Vec2::splat(qr_px), Sense::hover());
                                    let p = ui.painter();
                                    if let Ok(code) = qrcode::QrCode::new(url.as_bytes()) {
                                        let width = code.width();
                                        let cell = qr_px / width as f32;
                                        for y in 0..width {
                                            for x in 0..width {
                                                if code[(x, y)] == qrcode::Color::Dark {
                                                    let min = Pos2::new(
                                                        qr_rect.left() + x as f32 * cell,
                                                        qr_rect.top() + y as f32 * cell,
                                                    );
                                                    p.rect_filled(
                                                        Rect::from_min_max(
                                                            min,
                                                            Pos2::new(min.x + cell + 0.35, min.y + cell + 0.35),
                                                        ),
                                                        0.0,
                                                        Color32::from_rgb(14, 18, 28),
                                                    );
                                                }
                                            }
                                        }
                                    }
                                });

                            ui.vertical(|ui| {
                                ui.label(
                                    RichText::new("Control Your TV From Your Phone")
                                        .font(FontId::proportional(17.0))
                                        .strong()
                                        .color(Color32::WHITE),
                                );
                                ui.add_space(8.0);
                                ui.label(
                                    RichText::new(
                                        "Scan the QR code on the same Wi-Fi to:\n• Search & set any Weather City worldwide\n• Upload custom 4K wallpapers from your gallery\n• Clear RAM, switch themes & toggle HUD Overlay\n• Download or restore full JSON settings backups",
                                    )
                                    .font(FontId::proportional(12.5))
                                    .color(Color32::from_rgb(200, 214, 236)),
                                );
                                ui.add_space(12.0);
                                egui::Frame::NONE
                                    .fill(Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 32))
                                    .stroke(Stroke::new(1.0_f32, accent))
                                    .corner_radius(CornerRadius::same(10))
                                    .inner_margin(egui::Margin::symmetric(12, 7))
                                    .show(ui, |ui| {
                                        ui.label(
                                            RichText::new(&url)
                                                .font(FontId::proportional(13.5))
                                                .strong()
                                                .color(Color32::WHITE),
                                        );
                                    });
                            });
                        });
                    },
                );
            }

            ActiveModal::Backup => {
                let mut action: Option<u8> = None;
                if allow_keys {
                    ctx.input(|i| {
                        if i.key_pressed(Key::ArrowLeft) {
                            self.modal_focus = self.modal_focus.saturating_sub(1);
                        } else if i.key_pressed(Key::ArrowRight) {
                            self.modal_focus = (self.modal_focus + 1).min(1);
                        } else if i.key_pressed(Key::ArrowUp) {
                            self.modal_close_focused = true;
                        } else if i.key_pressed(Key::Enter) || i.key_pressed(Key::Space) {
                            action = Some(self.modal_focus as u8);
                        }
                    });
                }

                self.draw_modal_shell(
                    ctx,
                    "Backup & Restore  ·  Launcher Configuration JSON",
                    560.0,
                    250.0,
                    |this, ui, _ctx| {
                        let accent = this.config.accent();
                        ui.label(
                            RichText::new(
                                "Save or restore all pinned favorites, custom app names, folders, themes, weather city, and HUD overlay settings.",
                            )
                            .font(FontId::proportional(13.0))
                            .color(Color32::from_rgb(200, 214, 236)),
                        );
                        ui.add_space(10.0);
                        ui.label(
                            RichText::new("Tip: You can also download/upload JSON backups from your phone via Phone Companion.")
                                .font(FontId::proportional(11.5))
                                .color(Color32::from_rgb(165, 184, 212)),
                        );
                        ui.add_space(18.0);
                        ui.horizontal(|ui| {
                            if Self::modal_btn(ui, "Export Backup", true, this.modal_focus == 0, accent) {
                                action = Some(0);
                            }
                            ui.add_space(8.0);
                            if Self::modal_btn(ui, "Restore Backup", false, this.modal_focus == 1, accent) {
                                action = Some(1);
                            }
                        });
                    },
                );

                if let Some(a) = action {
                    if a == 0 {
                        if let Ok(json) = serde_json::to_string_pretty(&self.config) {
                            let res = bridge::export_backup(&json);
                            if res.starts_with("ERR:") {
                                self.show_toast(format!("Backup error: {res}"));
                            } else {
                                self.show_toast("✓ Exported astryx_launcher_backup.json");
                            }
                        }
                    } else {
                        let raw = bridge::import_backup();
                        if raw.is_empty() {
                            self.show_toast("No backup file found yet");
                        } else if let Ok(cfg) = serde_json::from_str::<LauncherConfig>(&raw) {
                            self.config = cfg;
                            self.save_config();
                            self.spawn_load_weather(ctx.clone());
                            self.ensure_wallpaper(ctx.clone());
                            self.show_toast("✓ Restored launcher backup!");
                            self.close_modal_step();
                        } else {
                            self.show_toast("Backup file is invalid JSON");
                        }
                    }
                }
            }

            ActiveModal::StartupOptions => {
                const ROW_COUNT: usize = 4;
                let mut trigger = false;
                if allow_keys {
                    ctx.input(|i| {
                        if i.key_pressed(Key::ArrowDown) {
                            self.modal_focus = (self.modal_focus + 1).min(ROW_COUNT - 1);
                            self.modal_focus_changed = true;
                        } else if i.key_pressed(Key::ArrowUp) {
                            if self.modal_focus == 0 {
                                self.modal_close_focused = true;
                            } else {
                                self.modal_focus -= 1;
                                self.modal_focus_changed = true;
                            }
                        } else if i.key_pressed(Key::Enter)
                            || i.key_pressed(Key::Space)
                            || i.key_pressed(Key::ArrowLeft)
                            || i.key_pressed(Key::ArrowRight)
                        {
                            trigger = true;
                        }
                    });
                }

                let app_name = if self.config.startup_app_pkg.is_empty() {
                    "None selected".to_string()
                } else {
                    self.apps
                        .iter()
                        .find(|a| a.package_name == self.config.startup_app_pkg)
                        .map(|a| self.display_label(a))
                        .unwrap_or_else(|| self.config.startup_app_pkg.clone())
                };

                let mut apply_idx: Option<usize> = if trigger { Some(self.modal_focus) } else { None };

                self.draw_modal_shell(
                    ctx,
                    "Startup Options  ·  Action when TV turns on",
                    640.0,
                    365.0,
                    |this, ui, _ctx| {
                        let accent = this.config.accent();
                        ui.label(
                            RichText::new(
                                "Choose what Astryx opens automatically when your TV boots up. Press BACK during the startup countdown to stay on Home.",
                            )
                            .font(FontId::proportional(12.0))
                            .color(Color32::from_rgb(185, 202, 226)),
                        );
                        ui.add_space(10.0);

                        let rows: [(&str, &str, String, usize); ROW_COUNT] = [
                            (
                                "On TV Turn-On",
                                "Stay on Home, launch a chosen app, resume last app, or open TV/HDMI input",
                                this.config.startup_mode.label().to_string(),
                                100,
                            ),
                            (
                                "Chosen Startup App",
                                "Press OK to cycle through installed apps",
                                app_name.clone(),
                                201,
                            ),
                            (
                                "Startup Delay",
                                "Grace period before auto-launching (press BACK to cancel)",
                                if this.config.startup_delay_secs == 0 {
                                    "Instant".to_string()
                                } else {
                                    format!("{}s", this.config.startup_delay_secs)
                                },
                                3,
                            ),
                            (
                                "Test Startup Action Now",
                                "Run your configured startup action immediately",
                                "Run ›".to_string(),
                                14,
                            ),
                        ];

                        for (idx, (title, sub, val, ic)) in rows.iter().enumerate() {
                            let focused = this.modal_focus == idx;
                            let resp = egui::Frame::NONE
                                .fill(if focused {
                                    Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 48)
                                } else {
                                    Color32::from_rgba_unmultiplied(255, 255, 255, 10)
                                })
                                .stroke(if focused {
                                    Stroke::new(1.8_f32, accent)
                                } else {
                                    Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 18))
                                })
                                .corner_radius(CornerRadius::same(12))
                                .inner_margin(egui::Margin::symmetric(14, 8))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        let (ir, _) = ui.allocate_exact_size(Vec2::splat(26.0), Sense::hover());
                                        let sc = Self::setting_icon_color(*ic);
                                        ui.painter().circle_filled(
                                            ir.center(),
                                            12.5,
                                            if focused {
                                                Color32::from_rgba_unmultiplied(255, 255, 255, 32)
                                            } else {
                                                Color32::from_rgba_unmultiplied(255, 255, 255, 16)
                                            },
                                        );
                                        Self::draw_setting_icon(ui.painter(), ir.center(), *ic, sc);
                                        ui.add_space(6.0);
                                        ui.vertical(|ui| {
                                            ui.label(RichText::new(*title).font(FontId::proportional(13.5)).strong().color(Color32::WHITE));
                                            ui.label(RichText::new(*sub).font(FontId::proportional(10.5)).color(Color32::from_rgb(172, 188, 214)));
                                        });
                                        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                                            egui::Frame::NONE
                                                .fill(Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), if focused { 95 } else { 38 }))
                                                .corner_radius(CornerRadius::same(8))
                                                .inner_margin(egui::Margin::symmetric(10, 4))
                                                .show(ui, |ui| {
                                                    ui.label(RichText::new(val).font(FontId::proportional(11.5)).strong().color(Color32::WHITE));
                                                });
                                        });
                                    });
                                })
                                .response;
                            if resp.clicked() {
                                this.modal_focus = idx;
                                apply_idx = Some(idx);
                            }
                            ui.add_space(6.0);
                        }
                    },
                );

                if let Some(idx) = apply_idx {
                    match idx {
                        0 => {
                            self.config.startup_mode = self.config.startup_mode.next();
                            self.save_config();
                        }
                        1 => {
                            if !visible.is_empty() {
                                let cur = visible
                                    .iter()
                                    .position(|a| a.package_name == self.config.startup_app_pkg);
                                let next_pkg = match cur {
                                    None => visible[0].package_name.clone(),
                                    Some(i) if i + 1 < visible.len() => {
                                        visible[i + 1].package_name.clone()
                                    }
                                    _ => String::new(),
                                };
                                self.config.startup_app_pkg = next_pkg;
                                self.save_config();
                            }
                        }
                        2 => {
                            self.config.startup_delay_secs = match self.config.startup_delay_secs {
                                0 => 2,
                                2 => 5,
                                _ => 0,
                            };
                            self.save_config();
                        }
                        3 => match self.config.startup_mode {
                            StartupMode::Home => self.show_toast("Startup action is set to Stay on Home"),
                            StartupMode::LaunchApp => {
                                let pkg = self.config.startup_app_pkg.clone();
                                if pkg.is_empty() {
                                    self.show_toast("Pick a Chosen Startup App first");
                                } else {
                                    self.modal = ActiveModal::None;
                                    self.modal_stack.clear();
                                    self.launch_app(&pkg);
                                }
                            }
                            StartupMode::LastUsedApp => {
                                if let Some(pkg) = self.config.recents.first().cloned() {
                                    self.modal = ActiveModal::None;
                                    self.modal_stack.clear();
                                    self.launch_app(&pkg);
                                } else {
                                    self.show_toast("No recently used app yet");
                                }
                            }
                            StartupMode::HdmiInput => {
                                bridge::open_tv_inputs();
                            }
                        },
                        _ => {}
                    }
                }
            }
        }
    }
    fn draw_zenith_home(&mut self, ctx: &egui::Context, visible: &[TvAppInfo]) {
        let accent = self.config.accent();
        let dock_fill = self.config.dock_fill();
        let screen = ctx.screen_rect();

        // 1. Top-Left Date & Time Capsule + Top-Right Weather & 7 Quick Icons Capsule
        if self.config.show_header {
            let time_str = self.cached_time_str.clone();
            let date_short = self.cached_date_short.clone();

            egui::Area::new(egui::Id::new("zenith_clock_top_left"))
                .anchor(Align2::LEFT_TOP, Vec2::new(28.0, 18.0))
                .order(egui::Order::Middle)
                .show(ctx, |ui| {
                    egui::Frame::NONE
                        .fill(dock_fill)
                        .stroke(Stroke::new(
                            1.0_f32,
                            Color32::from_rgba_unmultiplied(255, 255, 255, 48),
                        ))
                        .corner_radius(CornerRadius::same(26))
                        .inner_margin(egui::Margin::symmetric(18, 9))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 10.0;
                                ui.label(
                                    RichText::new(&time_str)
                                        .font(FontId::proportional(21.0))
                                        .strong()
                                        .color(Color32::WHITE),
                                );
                                ui.label(
                                    RichText::new(&date_short)
                                        .font(FontId::proportional(13.0))
                                        .color(Color32::from_rgba_unmultiplied(215, 226, 244, 215)),
                                );
                            });
                        });
                });

            egui::Area::new(egui::Id::new("zenith_top_bar"))
                .anchor(Align2::RIGHT_TOP, Vec2::new(-28.0, 18.0))
                .order(egui::Order::Middle)
                .show(ctx, |ui| {
                    egui::Frame::NONE
                        .fill(dock_fill)
                        .stroke(Stroke::new(
                            1.0_f32,
                            Color32::from_rgba_unmultiplied(255, 255, 255, 48),
                        ))
                        .corner_radius(CornerRadius::same(26))
                        .inner_margin(egui::Margin::symmetric(16, 8))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 12.0;

                                // Interactive Weather chip
                                if self.config.show_weather {
                                    let w_focused = self.modal == ActiveModal::None
                                        && self.focus == FocusTarget::Weather;
                                    let city_short = self
                                        .config
                                        .weather_city
                                        .split('·')
                                        .next()
                                        .unwrap_or("London")
                                        .trim();
                                    let w_txt = if let Some(w) = &self.weather {
                                        format!("{} {} · {city_short} ›", self.fmt_temp(w.temp_c), w.condition)
                                    } else {
                                        format!("{city_short} Weather ›")
                                    };
                                    let w_fill = if w_focused {
                                        Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 72)
                                    } else {
                                        Color32::from_rgba_unmultiplied(255, 255, 255, 14)
                                    };
                                    let w_stroke = if w_focused {
                                        Stroke::new(1.8_f32, accent)
                                    } else {
                                        Stroke::NONE
                                    };
                                    if ui
                                        .add(
                                            egui::Button::new(
                                                RichText::new(w_txt)
                                                    .font(FontId::proportional(13.0))
                                                    .strong()
                                                    .color(Color32::WHITE),
                                            )
                                            .fill(w_fill)
                                            .stroke(w_stroke)
                                            .corner_radius(CornerRadius::same(14)),
                                        )
                                        .clicked()
                                    {
                                        self.focus = FocusTarget::Weather;
                                        self.open_modal(ActiveModal::Weather);
                                    }
                                }

                                ui.separator();

                                // Crisp vector quick-action icons (includes Gemini AI sparkle when enabled)
                                ui.spacing_mut().item_spacing.x = 6.0;
                                let items = self.dock_items();
                                for (pos, (action_id, tip)) in items.iter().copied().enumerate() {
                                    let focused = self.modal == ActiveModal::None
                                        && self.focus == FocusTarget::Dock(pos);
                                    let anim = ctx.animate_bool_with_time(
                                        egui::Id::new(("zenith_dock_focus", action_id)),
                                        focused,
                                        0.12,
                                    );
                                    let (rect, resp) =
                                        ui.allocate_exact_size(Vec2::splat(32.0), Sense::click());
                                    let p = ui.painter();
                                    let r = 14.5 + anim * 1.8;
                                    if anim > 0.01 {
                                        p.circle_filled(
                                            rect.center(),
                                            r,
                                            Color32::from_rgba_unmultiplied(
                                                accent.r(),
                                                accent.g(),
                                                accent.b(),
                                                (anim * 75.0) as u8,
                                            ),
                                        );
                                        p.circle_stroke(
                                            rect.center(),
                                            r,
                                            Stroke::new(1.0_f32 + anim, accent),
                                        );
                                    } else {
                                        p.circle_filled(
                                            rect.center(),
                                            14.5,
                                            Color32::from_rgba_unmultiplied(255, 255, 255, 16),
                                        );
                                    }
                                    let icon_color = if action_id == 1 {
                                        if self.status.is_vpn_active {
                                            if focused {
                                                Color32::from_rgb(74, 222, 128)
                                            } else {
                                                Color32::from_rgb(52, 211, 153)
                                            }
                                        } else if focused {
                                            Color32::from_rgb(252, 138, 138)
                                        } else {
                                            Color32::from_rgb(248, 113, 113)
                                        }
                                    } else if action_id == 7 && !focused {
                                        accent
                                    } else if focused {
                                        Color32::WHITE
                                    } else {
                                        Color32::from_rgb(225, 232, 245)
                                    };
                                    Self::draw_dock_icon(p, rect.center(), action_id, icon_color);
                                    if action_id == 0 {
                                        let dot_c = if self.status.is_connected {
                                            Color32::from_rgb(52, 211, 153)
                                        } else {
                                            Color32::from_rgb(248, 113, 113)
                                        };
                                        p.circle_filled(
                                            rect.right_bottom() + Vec2::new(-4.5, -4.5),
                                            3.0,
                                            dot_c,
                                        );
                                    } else if action_id == 1 {
                                        let dot_c = if self.status.is_vpn_active {
                                            Color32::from_rgb(52, 211, 153)
                                        } else {
                                            Color32::from_rgb(248, 113, 113)
                                        };
                                        p.circle_filled(
                                            rect.right_bottom() + Vec2::new(-4.5, -4.5),
                                            3.0,
                                            dot_c,
                                        );
                                    } else if action_id == 6 && self.has_update_available() {
                                        p.circle_filled(
                                            rect.right_top() + Vec2::new(-4.0, 4.0),
                                            3.6,
                                            Color32::from_rgb(52, 211, 153),
                                        );
                                    }
                                    if resp.clicked() {
                                        self.focus = FocusTarget::Dock(pos);
                                        self.trigger_dock_action(action_id, ctx);
                                    }
                                    resp.on_hover_text(tip);
                                }
                            });
                        });
                });
        }

        // 2. Open Horizon Center: Active App Title & Optional A2UI Surfaces
        let focused_idx = match self.focus {
            FocusTarget::App(i) if i < visible.len() => Some(i),
            _ => None,
        };
        if let Some(idx) = focused_idx {
            let app = &visible[idx];
            let is_fav = self.config.favorites.contains(&app.package_name);
            let sub = if is_fav {
                format!("PINNED FAVORITE  •  {} of {}", idx + 1, visible.len())
            } else {
                format!("ALL APPS  •  {} of {}", idx + 1, visible.len())
            };
            egui::Area::new(egui::Id::new("zenith_hero_title"))
                .anchor(Align2::CENTER_BOTTOM, Vec2::new(0.0, -178.0))
                .order(egui::Order::Middle)
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.label(
                            RichText::new(&sub)
                                .font(FontId::proportional(11.5))
                                .strong()
                                .color(accent),
                        );
                        ui.add_space(2.0);
                        ui.label(
                            RichText::new(self.display_label(app))
                                .font(FontId::proportional(32.0))
                                .strong()
                                .color(Color32::WHITE),
                        );
                    });
                });
        }

        // Optional A2UI Surfaces in center-top horizon if injected
        if !self.a2ui.surfaces.is_empty() {
            egui::Area::new(egui::Id::new("zenith_a2ui_area"))
                .anchor(Align2::CENTER_TOP, Vec2::new(0.0, 86.0))
                .order(egui::Order::Middle)
                .show(ctx, |ui| {
                    self.draw_a2ui_surfaces(ui);
                });
        }

        // 3. Floating Liquid-Glass Bottom Dock with Shared-Element Sliding Halo
        let icon_d = (self.config.icon_size.px() * 0.84).clamp(54.0, 74.0);
        let slot_w = icon_d + 28.0;
        let slot_h = if self.config.show_labels {
            icon_d + 36.0
        } else {
            icon_d + 16.0
        };
        let dock_max_w = (screen.width() - 56.0).max(320.0);
        let mut to_launch: Option<String> = None;
        let mut to_context: Option<String> = None;

        egui::Area::new(egui::Id::new("zenith_bottom_dock"))
            .anchor(Align2::CENTER_BOTTOM, Vec2::new(0.0, -44.0))
            .order(egui::Order::Middle)
            .show(ctx, |ui| {
                egui::Frame::NONE
                    .fill(dock_fill)
                    .stroke(Stroke::new(
                        1.0_f32,
                        Color32::from_rgba_unmultiplied(255, 255, 255, 44),
                    ))
                    .corner_radius(CornerRadius::same(32))
                    .inner_margin(egui::Margin::symmetric(16, 10))
                    .show(ui, |ui| {
                        ui.set_max_width(dock_max_w);
                        ScrollArea::horizontal()
                            .id_salt("zenith_dock_scroll")
                            .auto_shrink([true, true])
                            .show(ui, |ui| {
                                let total_w = (visible.len() as f32 * (slot_w + 6.0)).max(slot_w);
                                let (strip_rect, _) =
                                    ui.allocate_exact_size(Vec2::new(total_w, slot_h), Sense::hover());

                                // Shared-element sliding liquid-glass halo behind focused icon!
                                if let Some(fidx) = focused_idx {
                                    let target_x = strip_rect.left()
                                        + fidx as f32 * (slot_w + 6.0)
                                        + slot_w * 0.5;
                                    let anim_x = ctx.animate_value_with_time(
                                        egui::Id::new("zenith_sliding_halo_x"),
                                        target_x,
                                        0.065,
                                    );
                                    let halo_rect = Rect::from_center_size(
                                        Pos2::new(anim_x, strip_rect.center().y),
                                        Vec2::new(slot_w - 2.0, slot_h),
                                    );
                                    let p = ui.painter();
                                    p.rect_filled(
                                        halo_rect,
                                        CornerRadius::same(22),
                                        Color32::from_rgba_unmultiplied(
                                            accent.r(),
                                            accent.g(),
                                            accent.b(),
                                            52,
                                        ),
                                    );
                                    p.rect_stroke(
                                        halo_rect,
                                        CornerRadius::same(22),
                                        Stroke::new(2.0_f32, accent),
                                        egui::StrokeKind::Inside,
                                    );
                                }

                                // Draw each circular app icon inside the strip
                                for (idx, app) in visible.iter().enumerate() {
                                    self.request_icon(&app.package_name, ctx);
                                    let x0 = strip_rect.left() + idx as f32 * (slot_w + 6.0);
                                    let slot_rect = Rect::from_min_size(
                                        Pos2::new(x0, strip_rect.top()),
                                        Vec2::new(slot_w, slot_h),
                                    );
                                    let resp = ui.interact(
                                        slot_rect,
                                        egui::Id::new(("zenith_slot", idx)),
                                        Sense::click(),
                                    );
                                    let is_focused = self.modal == ActiveModal::None
                                        && self.focus == FocusTarget::App(idx);
                                    if is_focused && self.focus_changed {
                                        ui.scroll_to_rect(slot_rect.expand(36.0), Some(Align::Center));
                                        self.focus_changed = false;
                                    }

                                    let t = ctx.animate_bool_with_time(
                                        egui::Id::new(("zenith_tile_lift", idx)),
                                        is_focused,
                                        0.065,
                                    );
                                    // Elastic Magnetic Wave: gently lift & scale immediate neighbors in Zenith dock
                                    let is_neighbor = self.config.focus_anim == FocusAnimStyle::MagneticWave
                                        && focused_idx
                                            .map(|fi| fi.abs_diff(idx) == 1)
                                            .unwrap_or(false);
                                    let nt = ctx.animate_bool_with_time(
                                        egui::Id::new(("zenith_wave_n", idx)),
                                        is_neighbor,
                                        0.08,
                                    );
                                    let (float_y, breath) = self.resting_focus_motion(is_focused, t);
                                    let base_center = Pos2::new(
                                        slot_rect.center().x,
                                        slot_rect.top() + icon_d * 0.5 + 8.0 - t * 4.0 - nt * 2.0,
                                    );
                                    let center = Pos2::new(base_center.x, base_center.y - float_y);
                                    let radius = icon_d * (0.46 + t * 0.065 + nt * 0.03 + breath * 0.008);
                                    let p = ui.painter();

                                    // Breathing resting aura + active focus animation effect
                                    if breath > 0.01 {
                                        p.circle_filled(
                                            center,
                                            radius + 7.5 + breath * 3.5,
                                            Color32::from_rgba_unmultiplied(
                                                accent.r(),
                                                accent.g(),
                                                accent.b(),
                                                (breath * 42.0) as u8,
                                            ),
                                        );
                                    }
                                    self.draw_focus_effects(ctx, p, center, radius, is_focused, accent);

                                    // Grounded drop shadow
                                    p.circle_filled(
                                        base_center + Vec2::new(0.0, 4.0 + t * 2.5),
                                        radius + 1.0 + breath * 1.8,
                                        Color32::from_rgba_unmultiplied(
                                            0,
                                            0,
                                            0,
                                            (85.0 + t * 48.0 - breath * 14.0) as u8,
                                        ),
                                    );

                                    if let Some(tex) = self.icon_textures.get(&app.package_name) {
                                        p.image(
                                            tex.id(),
                                            Rect::from_center_size(center, Vec2::splat(radius * 2.0)),
                                            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                                            Color32::WHITE,
                                        );
                                    } else {
                                        p.circle_filled(center, radius, Color32::from_rgb(36, 46, 68));
                                        let initial = app
                                            .label
                                            .chars()
                                            .next()
                                            .unwrap_or('A')
                                            .to_uppercase()
                                            .to_string();
                                        p.text(
                                            center,
                                            Align2::CENTER_CENTER,
                                            initial,
                                            FontId::proportional(radius * 0.72),
                                            Color32::WHITE,
                                        );
                                    }

                                    p.circle_stroke(
                                        center,
                                        radius,
                                        if is_focused {
                                            Stroke::new(2.4_f32, Color32::WHITE)
                                        } else {
                                            Stroke::new(
                                                1.2_f32,
                                                Color32::from_rgba_unmultiplied(255, 255, 255, 55),
                                            )
                                        },
                                    );
                                    self.draw_resting_focus_overlay(ctx, p, center, radius, is_focused, t, accent);

                                    // Golden star dot badge on top-right of pinned favorite icons
                                    if self.config.favorites.contains(&app.package_name) {
                                        let badge_c = center + Vec2::new(radius * 0.68, -radius * 0.68);
                                        p.circle_filled(badge_c, 6.2, Color32::from_rgb(16, 20, 32));
                                        p.circle_filled(badge_c, 4.4, Color32::from_rgb(250, 204, 21));
                                    }

                                    if self.config.show_labels {
                                        let mut lbl = self.display_label(app);
                                        if lbl.chars().count() > 11 {
                                            lbl = format!("{}…", lbl.chars().take(10).collect::<String>());
                                        }
                                        p.text(
                                            Pos2::new(slot_rect.center().x, center.y + radius + 12.0),
                                            Align2::CENTER_CENTER,
                                            lbl,
                                            FontId::proportional(if is_focused { 11.5 } else { 10.5 }),
                                            if is_focused {
                                                Color32::WHITE
                                            } else {
                                                Color32::from_rgba_unmultiplied(220, 230, 245, 200)
                                            },
                                        );
                                    }

                                    if resp.clicked() {
                                        self.focus = FocusTarget::App(idx);
                                        to_launch = Some(app.package_name.clone());
                                    } else if resp.secondary_clicked() {
                                        self.focus = FocusTarget::App(idx);
                                        to_context = Some(app.package_name.clone());
                                    }
                                }
                            });
                    });
            });

        if let Some(pkg) = to_launch {
            self.launch_app(&pkg);
        }
        if let Some(pkg) = to_context {
            self.open_modal(ActiveModal::AppContext(pkg));
        }
    }

    fn draw_ambient_screensaver(&self, ctx: &egui::Context) {
        let time_str = self.cached_time_str.clone();
        let date_str = self.cached_date_str.clone();
        let accent = self.config.accent();
        let accent2 = self.config.accent2();
        let w_line = self
            .weather
            .as_ref()
            .map(|w| format!("{}  ·  {}", self.fmt_temp(w.temp_c), w.condition))
            .unwrap_or_default();

        egui::Area::new(egui::Id::new("ambient_screensaver"))
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| match self.config.screensaver_style {
                ScreensaverStyle::Classic => {
                    ui.vertical_centered(|ui| {
                        ui.label(
                            RichText::new(&time_str)
                                .font(FontId::proportional(92.0))
                                .strong()
                                .color(Color32::from_rgba_unmultiplied(255, 255, 255, 238)),
                        );
                        ui.label(
                            RichText::new(&date_str)
                                .font(FontId::proportional(20.0))
                                .color(Color32::from_rgba_unmultiplied(215, 228, 246, 200)),
                        );
                        if !w_line.is_empty() {
                            ui.add_space(10.0);
                            ui.label(
                                RichText::new(&w_line)
                                    .font(FontId::proportional(18.0))
                                    .strong()
                                    .color(accent),
                            );
                        }
                    });
                }

                ScreensaverStyle::GlassCard => {
                    egui::Frame::NONE
                        .fill(Color32::from_rgba_unmultiplied(10, 14, 26, 188))
                        .stroke(Stroke::new(
                            1.5_f32,
                            Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 145),
                        ))
                        .corner_radius(CornerRadius::same(28))
                        .inner_margin(egui::Margin::symmetric(48, 34))
                        .show(ui, |ui| {
                            ui.vertical_centered(|ui| {
                                ui.label(
                                    RichText::new(&time_str)
                                        .font(FontId::proportional(84.0))
                                        .strong()
                                        .color(Color32::WHITE),
                                );
                                ui.add_space(6.0);
                                let (bar_r, _) =
                                    ui.allocate_exact_size(Vec2::new(110.0, 2.0), Sense::hover());
                                ui.painter()
                                    .rect_filled(bar_r, CornerRadius::same(1), accent);
                                ui.add_space(8.0);
                                ui.label(
                                    RichText::new(&date_str)
                                        .font(FontId::proportional(18.5))
                                        .color(Color32::from_rgb(208, 220, 240)),
                                );
                                if !w_line.is_empty() {
                                    ui.add_space(6.0);
                                    ui.label(
                                        RichText::new(format!(
                                            "{}  ·  {}",
                                            self.config.weather_city, w_line
                                        ))
                                        .font(FontId::proportional(15.5))
                                        .strong()
                                        .color(accent),
                                    );
                                }
                            });
                        });
                }

                ScreensaverStyle::MinimalClock => {
                    ui.vertical_centered(|ui| {
                        ui.label(
                            RichText::new(&time_str)
                                .font(FontId::proportional(118.0))
                                .strong()
                                .color(Color32::from_rgba_unmultiplied(255, 255, 255, 242)),
                        );
                    });
                }

                ScreensaverStyle::EditorialSplit => {
                    egui::Frame::NONE
                        .fill(Color32::from_rgba_unmultiplied(8, 12, 22, 152))
                        .corner_radius(CornerRadius::same(24))
                        .inner_margin(egui::Margin::symmetric(36, 24))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 22.0;
                                ui.label(
                                    RichText::new(&time_str)
                                        .font(FontId::proportional(82.0))
                                        .strong()
                                        .color(Color32::WHITE),
                                );
                                let (div_r, _) =
                                    ui.allocate_exact_size(Vec2::new(3.0, 74.0), Sense::hover());
                                ui.painter()
                                    .rect_filled(div_r, CornerRadius::same(2), accent);
                                ui.vertical(|ui| {
                                    ui.add_space(8.0);
                                    ui.label(
                                        RichText::new(date_str.to_uppercase())
                                            .font(FontId::proportional(16.0))
                                            .strong()
                                            .color(Color32::from_rgb(226, 236, 250)),
                                    );
                                    ui.add_space(4.0);
                                    ui.label(
                                        RichText::new(&self.config.weather_city)
                                            .font(FontId::proportional(14.5))
                                            .color(Color32::from_rgb(164, 180, 206)),
                                    );
                                    if !w_line.is_empty() {
                                        ui.add_space(4.0);
                                        ui.label(
                                            RichText::new(&w_line)
                                                .font(FontId::proportional(16.5))
                                                .strong()
                                                .color(accent),
                                        );
                                    }
                                });
                            });
                        });
                }

                ScreensaverStyle::NeonRing => {
                    let dial_size = Vec2::splat(290.0);
                    let (rect, _) = ui.allocate_exact_size(dial_size, Sense::hover());
                    let p = ui.painter();
                    let c = rect.center();
                    let radius = 132.0_f32;
                    p.circle_filled(
                        c,
                        radius - 6.0,
                        Color32::from_rgba_unmultiplied(8, 12, 24, 178),
                    );
                    p.circle_stroke(
                        c,
                        radius + 6.0,
                        Stroke::new(
                            1.2_f32,
                            Color32::from_rgba_unmultiplied(
                                accent2.r(),
                                accent2.g(),
                                accent2.b(),
                                85,
                            ),
                        ),
                    );
                    p.circle_stroke(
                        c,
                        radius,
                        Stroke::new(
                            2.6_f32,
                            Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 205),
                        ),
                    );
                    p.text(
                        c + Vec2::new(0.0, -18.0),
                        Align2::CENTER_CENTER,
                        &time_str,
                        FontId::proportional(58.0),
                        Color32::WHITE,
                    );
                    p.text(
                        c + Vec2::new(0.0, 24.0),
                        Align2::CENTER_CENTER,
                        &self.cached_date_short,
                        FontId::proportional(15.5),
                        Color32::from_rgb(206, 220, 242),
                    );
                    if !w_line.is_empty() {
                        p.text(
                            c + Vec2::new(0.0, 50.0),
                            Align2::CENTER_CENTER,
                            &w_line,
                            FontId::proportional(14.5),
                            accent,
                        );
                    }
                }
            });
    }
}

impl eframe::App for AstryxTvApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if bridge::is_paused() {
            return;
        }
        self.drain_messages(ctx);

        // Instant refresh when any app is installed, updated, or uninstalled
        if bridge::take_packages_changed() {
            self.pending_apps_refresh = false;
            self.spawn_load_apps(ctx.clone());
        }

        // Instant refresh on network/VPN change or Activity resume + 3s fallback poll
        let status_changed = bridge::take_status_refresh_requested();
        if status_changed || self.last_status_poll.elapsed() > Duration::from_secs(3) {
            self.spawn_load_status(ctx.clone());
            if status_changed && self.pending_apps_refresh {
                self.pending_apps_refresh = false;
                self.spawn_load_apps(ctx.clone());
            }
        }
        if self.last_weather_poll.elapsed() > Duration::from_secs(900) {
            self.spawn_load_weather(ctx.clone());
        }
        // Auto-refresh Live Satellite wallpapers (Live UK Sat / Live Earth) every 10 minutes
        let cur_scene = self.active_scene();
        if matches!(cur_scene, WeatherScene::LiveUkSat | WeatherScene::LiveEarth)
            && self.last_sat_refresh.elapsed() > Duration::from_secs(600)
        {
            self.last_sat_refresh = Instant::now();
            self.wallpaper_loading = None;
            self.wallpaper_tex = None;
            self.ensure_wallpaper(ctx.clone());
        }

        self.update_clock_cache();
        let screen = ctx.screen_rect();
        self.draw_background(ctx, screen);

        // Execute configured Startup Option (chosen app / last app / TV HDMI input) with cancel-on-BACK
        if self.startup_pending {
            let cancel = bridge::take_back_pressed()
                || ctx.input(|i| i.key_pressed(Key::Escape) || i.key_pressed(Key::Backspace));
            if cancel {
                self.startup_pending = false;
                self.show_toast("Startup auto-launch cancelled");
            } else if self.startup_started_at.elapsed()
                >= Duration::from_secs(self.config.startup_delay_secs as u64)
            {
                self.startup_pending = false;
                match self.config.startup_mode {
                    StartupMode::Home => {}
                    StartupMode::LaunchApp => {
                        let pkg = self.config.startup_app_pkg.clone();
                        if !pkg.is_empty() {
                            self.launch_app(&pkg);
                        }
                    }
                    StartupMode::LastUsedApp => {
                        if let Some(pkg) = self.config.recents.first().cloned() {
                            self.launch_app(&pkg);
                        }
                    }
                    StartupMode::HdmiInput => {
                        bridge::open_tv_inputs();
                    }
                }
            } else {
                ctx.request_repaint_after(Duration::from_millis(200));
            }
        }

        // Track user input for Ambient Clock Screensaver auto-idle
        let had_input = ctx.input(|i| {
            !i.events.is_empty()
                || i.pointer.any_pressed()
                || i.pointer.any_released()
        });
        if had_input {
            self.last_input_at = Instant::now();
            if self.ambient_active {
                self.ambient_active = false;
                return;
            }
        } else if !self.ambient_active
            && self.modal == ActiveModal::None
            && self.config.ambient_timeout_secs > 0
            && self.last_input_at.elapsed().as_secs() >= self.config.ambient_timeout_secs as u64
        {
            self.ambient_active = true;
        }

        // Ambient Clock Screensaver Mode (static centered display with selectable style)
        if self.ambient_active {
            self.draw_ambient_screensaver(ctx);
            ctx.request_repaint_after(Duration::from_millis(500));
            return;
        }

        let visible = Arc::clone(&self.cached_visible);
        let favs = Arc::clone(&self.cached_favs);

        // Handle D-pad keys on home screen when no modal is open
        if self.modal == ActiveModal::None {
            self.handle_home_keys(ctx, &favs, &visible);
        }

        if self.config.layout_style == LayoutStyle::Zenith {
            self.draw_zenith_home(ctx, &visible);
        } else {
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.inner_margin(egui::Margin::symmetric(28, 18)))
            .show(ctx, |ui| {
                self.draw_top_dock(ui, ctx);
                self.draw_hero_header(ui);

                let icon_d = self.config.icon_size.px();
                let tile_w = icon_d + 30.0;
                let avail_w = ui.available_width().max(tile_w);
                let cols = ((avail_w / (tile_w + 10.0)).floor() as usize).clamp(3, 10);
                self.last_grid_cols = cols;

                let mut to_launch: Option<String> = None;
                let mut to_context: Option<String> = None;

                ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        // A2UI Live Agent Surfaces (when injected)
                        self.draw_a2ui_surfaces(ui);

                        // Pinned Favorites Row (only if non-empty)
                        if !favs.is_empty() {
                            ui.label(
                                RichText::new("PINNED FAVORITES")
                                    .font(FontId::proportional(12.5))
                                    .strong()
                                    .color(Color32::from_rgba_unmultiplied(230, 238, 250, 210)),
                            );
                            ui.add_space(6.0);
                            ui.horizontal_wrapped(|ui| {
                                ui.spacing_mut().item_spacing = Vec2::new(10.0, 10.0);
                                for (idx, app) in favs.iter().enumerate() {
                                    let (clicked, rclick) = self.draw_app_tile(
                                        ui,
                                        ctx,
                                        app,
                                        FocusTarget::Fav(idx),
                                        tile_w,
                                        icon_d,
                                    );
                                    if clicked {
                                        self.focus = FocusTarget::Fav(idx);
                                        to_launch = Some(app.package_name.clone());
                                    } else if rclick {
                                        self.focus = FocusTarget::Fav(idx);
                                        to_context = Some(app.package_name.clone());
                                    }
                                }
                            });
                            ui.add_space(14.0);
                        }

                        // All Apps Grid (no categories)
                        ui.label(
                            RichText::new(format!("ALL APPS  ({})", visible.len()))
                                .font(FontId::proportional(12.5))
                                .strong()
                                .color(Color32::from_rgba_unmultiplied(230, 238, 250, 210)),
                        );
                        ui.add_space(6.0);

                        if visible.is_empty() {
                            ui.label(
                                RichText::new("Loading installed Android TV apps...")
                                    .font(FontId::proportional(15.0))
                                    .color(Color32::from_rgb(200, 214, 235)),
                            );
                        } else {
                            for (row_idx, chunk) in visible.chunks(cols).enumerate() {
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 10.0;
                                    for (c_idx, app) in chunk.iter().enumerate() {
                                        let idx = row_idx * cols + c_idx;
                                        let (clicked, rclick) = self.draw_app_tile(
                                            ui,
                                            ctx,
                                            app,
                                            FocusTarget::App(idx),
                                            tile_w,
                                            icon_d,
                                        );
                                        if clicked {
                                            self.focus = FocusTarget::App(idx);
                                            to_launch = Some(app.package_name.clone());
                                        } else if rclick {
                                            self.focus = FocusTarget::App(idx);
                                            to_context = Some(app.package_name.clone());
                                        }
                                    }
                                });
                                ui.add_space(6.0);
                            }
                        }

                        ui.add_space(36.0);
                    });

                if let Some(pkg) = to_launch {
                    self.launch_app(&pkg);
                }
                if let Some(pkg) = to_context {
                    self.open_modal(ActiveModal::AppContext(pkg));
                }
            });
        }

        // Interactive Clear RAM & Live Status Pill (bottom-left, D-pad Down from apps to focus!)
        let mut do_clear_ram = false;
        let mut do_play_pause = false;
        {
            let accent = self.config.accent();
            let ram_focused = self.modal == ActiveModal::None && self.focus == FocusTarget::RamPill;
            let net_txt = if self.status.is_connected {
                if self.status.network_type.is_empty() {
                    "Online"
                } else {
                    &self.status.network_type
                }
            } else {
                "Offline"
            };
            let vpn_txt = if self.status.is_vpn_active {
                "VPN ON"
            } else {
                "VPN OFF"
            };

            egui::Area::new(egui::Id::new("live_status_pill"))
                .anchor(Align2::LEFT_BOTTOM, Vec2::new(24.0, -12.0))
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;

                        // Interactive Clear RAM Pill with microchip vector icon
                        let pill_fill = if ram_focused {
                            Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 88)
                        } else {
                            Color32::from_rgba_unmultiplied(12, 16, 28, 175)
                        };
                        let pill_stroke = if ram_focused {
                            Stroke::new(2.0_f32, accent)
                        } else {
                            Stroke::new(
                                1.0_f32,
                                Color32::from_rgba_unmultiplied(255, 255, 255, 36),
                            )
                        };
                        let ram_lbl = if self.status.ram_total_mb > 0 {
                            format!("Clear RAM · {} MB", self.status.ram_free_mb)
                        } else {
                            "Clear RAM".to_string()
                        };
                        let resp = egui::Frame::NONE
                            .fill(pill_fill)
                            .stroke(pill_stroke)
                            .corner_radius(CornerRadius::same(14))
                            .inner_margin(egui::Margin::symmetric(10, 4))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 6.0;
                                    let (ir, _) = ui.allocate_exact_size(Vec2::splat(16.0), Sense::hover());
                                    Self::draw_setting_icon(
                                        ui.painter(),
                                        ir.center(),
                                        101,
                                        if ram_focused { Color32::WHITE } else { accent },
                                    );
                                    ui.label(
                                        RichText::new(ram_lbl)
                                            .font(FontId::proportional(11.5))
                                            .strong()
                                            .color(Color32::WHITE),
                                    );
                                });
                            })
                            .response
                            .interact(Sense::click());
                        if resp.clicked() {
                            self.focus = FocusTarget::RamPill;
                            do_clear_ram = true;
                        }

                        // Live Now Playing Pill (clickable to Play/Pause active media)
                        if self.config.show_now_playing && self.is_music_active {
                            let np_resp = egui::Frame::NONE
                                .fill(Color32::from_rgba_unmultiplied(
                                    accent.r(),
                                    accent.g(),
                                    accent.b(),
                                    58,
                                ))
                                .stroke(Stroke::new(1.2_f32, accent))
                                .corner_radius(CornerRadius::same(14))
                                .inner_margin(egui::Margin::symmetric(10, 4))
                                .show(ui, |ui| {
                                    ui.label(
                                        RichText::new("♫ Now Playing · Play/Pause")
                                            .font(FontId::proportional(11.5))
                                            .strong()
                                            .color(Color32::WHITE),
                                    );
                                })
                                .response
                                .interact(Sense::click());
                            if np_resp.clicked() {
                                do_play_pause = true;
                            }
                        }

                        // Compact Live Network & VPN Pill (when enabled in Settings)
                        if self.config.show_status_pill {
                            egui::Frame::NONE
                                .fill(Color32::from_rgba_unmultiplied(10, 14, 24, 155))
                                .stroke(Stroke::new(
                                    1.0_f32,
                                    Color32::from_rgba_unmultiplied(255, 255, 255, 26),
                                ))
                                .corner_radius(CornerRadius::same(14))
                                .inner_margin(egui::Margin::symmetric(10, 4))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing.x = 7.0;
                                        let net_c = if self.status.is_connected {
                                            Color32::from_rgb(52, 211, 153)
                                        } else {
                                            Color32::from_rgb(248, 113, 113)
                                        };
                                        ui.label(
                                            RichText::new(format!("● {net_txt}"))
                                                .font(FontId::proportional(11.0))
                                                .strong()
                                                .color(net_c),
                                        );
                                        ui.label(
                                            RichText::new("•")
                                                .font(FontId::proportional(11.0))
                                                .color(Color32::from_rgb(130, 145, 170)),
                                        );
                                        ui.label(
                                            RichText::new(vpn_txt)
                                                .font(FontId::proportional(11.0))
                                                .strong()
                                                .color(if self.status.is_vpn_active {
                                                    Color32::from_rgb(52, 211, 153)
                                                } else {
                                                    Color32::from_rgb(248, 113, 113)
                                                }),
                                        );
                                    });
                                });
                        }
                    });
                });
        }
        if do_clear_ram {
            self.spawn_clear_ram(ctx.clone());
        }
        if do_play_pause {
            bridge::media_play_pause();
            self.show_toast("Toggled Play / Pause");
        }

        // Subtle bottom-right remote control hint pill
        egui::Area::new(egui::Id::new("hint_pill"))
            .anchor(Align2::RIGHT_BOTTOM, Vec2::new(-24.0, -14.0))
            .show(ctx, |ui| {
                egui::Frame::NONE
                    .fill(Color32::from_rgba_unmultiplied(10, 14, 24, 150))
                    .corner_radius(CornerRadius::same(12))
                    .inner_margin(egui::Margin::symmetric(10, 4))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new("OK Open  •  Hold OK / MENU Options  •  BACK Close")
                                .font(FontId::proportional(11.0))
                                .color(Color32::from_rgba_unmultiplied(215, 226, 242, 200)),
                        );
                    });
            });

        // Pressing the TV remote HOME button dismisses any active modal/orb and returns cleanly to Home
        if bridge::take_home_pressed() {
            if self.gemini_listening {
                let _ = bridge::stop_voice_recording();
                self.gemini_listening = false;
            }
            bridge::stop_speaking();
            self.modal_stack.clear();
            self.modal = ActiveModal::None;
            self.modal_close_focused = false;
            self.ambient_active = false;
            self.last_input_at = Instant::now();
        }

        // Active modal overlay
        self.render_modals(ctx, &visible);

        // In-launcher FPS Meter Pill fallback (only shown if system-wide overlay isn't active yet)
        if self.config.show_fps_meter && !self.fps_overlay_active && self.config.overlay_flags() != 0 {
            self.fps_frame_count = self.fps_frame_count.saturating_add(1);
            let win_s = self.fps_window_start.elapsed().as_secs_f32();
            if win_s >= 0.45 {
                self.fps_current = (self.fps_frame_count as f32 / win_s).clamp(1.0, 240.0);
                self.fps_frame_count = 0;
                self.fps_window_start = Instant::now();
            }
            ctx.request_repaint();

            let fps_val = self.fps_current.round() as u32;
            let dot_c = if !self.config.overlay_show_fps {
                Color32::from_rgb(56, 189, 248)
            } else if self.fps_current >= (self.display_hz - 5.0).max(52.0) {
                Color32::from_rgb(52, 211, 153)
            } else if self.fps_current >= 35.0 {
                Color32::from_rgb(251, 191, 36)
            } else {
                Color32::from_rgb(248, 113, 113)
            };

            let mut parts: Vec<String> = Vec::new();
            if self.config.overlay_show_fps {
                parts.push(format!("{fps_val} FPS"));
            }
            if self.config.overlay_show_hz {
                parts.push(format!("{:.0}Hz", self.display_hz));
            }
            if self.config.overlay_show_ram {
                parts.push(format!("RAM {} MB", self.status.ram_free_mb));
            }
            let hud_txt = if parts.is_empty() {
                "HUD LIVE".to_string()
            } else {
                parts.join("  ·  ")
            };

            egui::Area::new(egui::Id::new("fps_meter_pill"))
                .anchor(Align2::CENTER_TOP, Vec2::new(0.0, 18.0))
                .order(egui::Order::Tooltip)
                .interactable(false)
                .show(ctx, |ui| {
                    egui::Frame::NONE
                        .fill(Color32::from_rgba_unmultiplied(12, 16, 28, 212))
                        .stroke(Stroke::new(
                            1.0_f32,
                            Color32::from_rgba_unmultiplied(255, 255, 255, 42),
                        ))
                        .corner_radius(CornerRadius::same(16))
                        .inner_margin(egui::Margin::symmetric(12, 5))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 6.0;
                                let (dr, _) =
                                    ui.allocate_exact_size(Vec2::splat(7.0), Sense::hover());
                                ui.painter().circle_filled(dr.center(), 3.2, dot_c);
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(hud_txt)
                                            .font(FontId::proportional(11.5))
                                            .strong()
                                            .color(Color32::WHITE),
                                    )
                                    .extend(),
                                );
                            });
                        });
                });
        }

        // Toast notification (single-line auto-sized frosted pill, clear of top bar & bottom dock)
        if let Some((msg, at)) = &self.toast {
            let elapsed = at.elapsed();
            if elapsed < Duration::from_secs(3) {
                let accent = self.config.accent();
                let fade = if elapsed.as_secs_f32() > 2.65 {
                    ((3.0 - elapsed.as_secs_f32()) / 0.35).clamp(0.0, 1.0)
                } else {
                    1.0
                };
                if fade < 1.0 {
                    ctx.request_repaint();
                } else {
                    ctx.request_repaint_after(Duration::from_millis(250));
                }
                let bg_a = (232.0 * fade) as u8;
                let st_a = (255.0 * fade) as u8;
                let tx_a = (255.0 * fade) as u8;

                egui::Area::new(egui::Id::new("toast"))
                    .anchor(Align2::CENTER_TOP, Vec2::new(0.0, 74.0))
                    .order(egui::Order::Tooltip)
                    .interactable(false)
                    .show(ctx, |ui| {
                        egui::Frame::NONE
                            .fill(Color32::from_rgba_unmultiplied(12, 16, 28, bg_a))
                            .stroke(Stroke::new(
                                1.5_f32,
                                Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), st_a),
                            ))
                            .corner_radius(CornerRadius::same(18))
                            .inner_margin(egui::Margin::symmetric(18, 9))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 8.0;
                                    let (dr, _) =
                                        ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
                                    ui.painter().circle_filled(
                                        dr.center(),
                                        3.8,
                                        Color32::from_rgba_unmultiplied(
                                            accent.r(),
                                            accent.g(),
                                            accent.b(),
                                            st_a,
                                        ),
                                    );
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(msg)
                                                .font(FontId::proportional(13.5))
                                                .strong()
                                                .color(Color32::from_rgba_unmultiplied(
                                                    255, 255, 255, tx_a,
                                                )),
                                        )
                                        .extend(),
                                    );
                                });
                            });
                    });
            } else {
                self.toast = None;
            }
        }
    }
}
