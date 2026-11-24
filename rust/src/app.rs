use crate::a2ui::{A2uiEngine, SAMPLE_A2UI_PAYLOAD};
use crate::bridge::{self, TvAppInfo, TvSystemStatus};
use crate::theme::{
    AstryxThemeId, FocusAnimStyle, IconSize, LauncherConfig, LayoutStyle, SettingsViewMode,
    SortOrder, WeatherScene, ACCENT_PRESETS,
};
use crate::updater::{self, ReleaseInfo, UpdateCheckOutcome};
use crate::weather::{self, WeatherData};
use chrono::{Local, Timelike};
use egui::{
    Align, Align2, Color32, ColorImage, CornerRadius, FontId, Key, Pos2, Rect, RichText,
    ScrollArea, Sense, Stroke, TextureHandle, TextureOptions, Ui, Vec2,
};
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

const PREF_CONFIG_KEY: &str = "astryx_rust_config_v1";

enum BgMsg {
    AppsLoaded(Vec<TvAppInfo>),
    IconLoaded(String, ColorImage),
    StatusLoaded(TvSystemStatus, bool),
    WeatherLoaded(WeatherData),
    WallpaperLoaded(WeatherScene, u8, ColorImage),
    RamCleared(i32, i32), // (before_mb, after_mb)
    UpdateChecked(UpdateCheckOutcome, bool), // (outcome, silent)
    UpdateDownloadProgress(u64, u64),
    UpdateDownloadDone(Result<String, String>),
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
    Search,
    A2ui,
    AppContext(String), // package_name
    RenameApp(String),  // package_name
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
    weather: Option<WeatherData>,
    use_fahrenheit: bool,

    // Textures
    icon_textures: HashMap<String, TextureHandle>,
    icon_loading: HashSet<String>,
    wallpaper_tex: Option<(WeatherScene, u8, TextureHandle)>,
    wallpaper_loading: Option<(WeatherScene, u8)>,

    // A2UI
    a2ui: A2uiEngine,
    a2ui_input: String,
    a2ui_feedback: String,

    // Internal OTA Updater
    app_version: String,
    updater_state: UpdaterState,

    // Navigation & Modals
    focus: FocusTarget,
    last_app_idx: usize,
    focus_changed: bool,
    focus_changed_at: Instant,
    modal: ActiveModal,
    modal_stack: Vec<(ActiveModal, usize)>,
    modal_focus: usize,
    modal_just_opened: bool,
    modal_wait_key_release: bool,
    modal_focus_changed: bool,
    modal_opened_at: Instant,
    last_back_handled: Instant,
    last_sat_refresh: Instant,
    settings_sys_col: usize,
    select_down_since: Option<Instant>,
    select_long_triggered: bool,
    search_query: String,
    rename_input: String,
    toast: Option<(String, Instant)>,

    // Async messaging & timers
    tx: Sender<BgMsg>,
    rx: Receiver<BgMsg>,
    start_time: Instant,
    last_status_poll: Instant,
    last_weather_poll: Instant,
    last_grid_cols: usize,
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
        let now = Instant::now();

        let show_welcome = !config.onboarding_done;

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
            weather: None,
            use_fahrenheit: false,
            icon_textures: HashMap::new(),
            icon_loading: HashSet::new(),
            wallpaper_tex: None,
            wallpaper_loading: None,
            a2ui: A2uiEngine::default(),
            a2ui_input: SAMPLE_A2UI_PAYLOAD.to_string(),
            a2ui_feedback: String::new(),
            app_version: bridge::get_app_version(),
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
            modal_just_opened: false,
            modal_wait_key_release: false,
            modal_focus_changed: false,
            modal_opened_at: now,
            last_back_handled: now - Duration::from_secs(2),
            last_sat_refresh: now,
            settings_sys_col: 0,
            select_down_since: None,
            select_long_triggered: false,
            search_query: String::new(),
            rename_input: String::new(),
            toast: None,
            tx,
            rx,
            start_time: now,
            last_status_poll: now - Duration::from_secs(60),
            last_weather_poll: now - Duration::from_secs(900),
            last_grid_cols: 6,
        };

        app.spawn_load_apps(cc.egui_ctx.clone());
        app.spawn_load_status(cc.egui_ctx.clone());
        app.spawn_load_weather(cc.egui_ctx.clone());
        app.ensure_wallpaper(cc.egui_ctx.clone());
        app.spawn_check_update(cc.egui_ctx.clone(), true);
        app
    }

    fn save_config(&mut self) {
        self.rebuild_app_lists();
        if let Ok(json) = serde_json::to_string(&self.config) {
            bridge::set_pref(PREF_CONFIG_KEY, &json);
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
            self.cached_date_str = format!("{}  •  London · UK", now.format("%A, %d %B %Y"));
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
        if modal == ActiveModal::None {
            self.modal_stack.clear();
        } else if self.modal != ActiveModal::None && self.modal != modal {
            self.modal_stack.push((self.modal.clone(), self.modal_focus));
        }
        self.modal = modal;
        self.modal_focus = 0;
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
        if let Some((prev_modal, prev_focus)) = self.modal_stack.pop() {
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
            let _ = tx.send(BgMsg::StatusLoaded(status, is_home));
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
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            if let Some(w) = weather::fetch_london_weather() {
                let _ = tx.send(BgMsg::WeatherLoaded(w));
                ctx.request_repaint();
            }
        });
    }

    fn ensure_wallpaper(&mut self, ctx: egui::Context) {
        let scene = self.active_scene();
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
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            if let Some(img) = weather::fetch_wallpaper_image(scene, blur) {
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
                BgMsg::StatusLoaded(st, is_home) => {
                    self.status = st;
                    self.is_default_home = is_home;
                }
                BgMsg::WeatherLoaded(w) => {
                    self.weather = Some(w);
                    self.ensure_wallpaper(ctx.clone());
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
            }
        }
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

        let has_weather = self.config.show_header && self.config.show_weather;
        let has_favs = !favs.is_empty();
        let app_len = visible.len();
        let cols = self.last_grid_cols.max(1);
        let prev = self.focus.clone();

        if select {
            match &self.focus {
                FocusTarget::Dock(idx) => match idx {
                    0 => bridge::open_system_settings("network"),
                    1 => self.open_vpn_app(),
                    2 => self.open_modal(ActiveModal::Search),
                    3 => self.open_modal(ActiveModal::Theme),
                    4 => self.open_modal(ActiveModal::A2ui),
                    5 => self.force_refresh_all(ctx),
                    6 => self.open_modal(ActiveModal::Settings),
                    _ => {}
                },
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
                if left {
                    if i > 0 {
                        self.focus = FocusTarget::Dock(i - 1);
                    } else if is_zenith && has_weather {
                        // In Zenith unified top bar, Weather sits directly left of Wi-Fi
                        self.focus = FocusTarget::Weather;
                    }
                    // In Classic mode, Left on Wi-Fi (0) stops strictly at edge!
                } else if right {
                    self.focus = FocusTarget::Dock((i + 1).min(6));
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
                    self.focus = FocusTarget::Dock(6);
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
                            self.focus = FocusTarget::Dock(col.min(6));
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
        if !focused {
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
                ctx.request_repaint_after(Duration::from_millis(33));
            }
            FocusAnimStyle::MagneticWave | FocusAnimStyle::Minimal => {}
        }
    }

    fn draw_background(&self, ctx: &egui::Context, rect: Rect) {
        let painter = ctx.layer_painter(egui::LayerId::background());
        let scene = self.active_scene();

        let t = self.start_time.elapsed().as_secs_f32();
        let motion = self.config.live_sky_motion && self.modal == ActiveModal::None;

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
                    ctx.request_repaint_after(Duration::from_millis(33));
                } else {
                    ctx.request_repaint_after(Duration::from_millis(80));
                }
            }
            _ => {
                // Smooth low-CPU cinematic UV sky drift (no hard-edged overlays)
                if motion {
                    ctx.request_repaint_after(Duration::from_millis(80));
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
            // System Settings shortcut: cog
            _ => Self::draw_dock_icon(p, c, 6, color),
        }
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
                // VPN shield
                let pts = vec![
                    center + Vec2::new(0.0, -7.5),
                    center + Vec2::new(6.5, -4.5),
                    center + Vec2::new(5.5, 2.5),
                    center + Vec2::new(0.0, 7.5),
                    center + Vec2::new(-5.5, 2.5),
                    center + Vec2::new(-6.5, -4.5),
                ];
                painter.add(egui::Shape::closed_line(pts, s));
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
                            let labels = [
                                "Wi-Fi", "VPN", "Search", "Theme", "A2UI", "Refresh", "Settings",
                            ];
                            for (idx, tip) in labels.iter().enumerate() {
                                let focused =
                                    self.modal == ActiveModal::None && self.focus == FocusTarget::Dock(idx);
                                let anim = ctx.animate_bool_with_time(
                                    egui::Id::new(("dock_focus", idx)),
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

                                let icon_color = if focused {
                                    Color32::WHITE
                                } else {
                                    Color32::from_rgb(225, 232, 245)
                                };
                                Self::draw_dock_icon(p, rect.center(), idx, icon_color);

                                // Live status indicator dots for Wi-Fi (0) and VPN (1)
                                if idx == 0 {
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
                                } else if idx == 1 && self.status.is_vpn_active {
                                    p.circle_filled(
                                        rect.right_bottom() + Vec2::new(-5.0, -5.0),
                                        3.2,
                                        Color32::from_rgb(56, 189, 248),
                                    );
                                } else if idx == 6 && self.has_update_available() {
                                    p.circle_filled(
                                        rect.right_top() + Vec2::new(-4.5, 4.5),
                                        3.8,
                                        Color32::from_rgb(52, 211, 153),
                                    );
                                }

                                if resp.clicked() {
                                    self.focus = FocusTarget::Dock(idx);
                                    match idx {
                                        0 => bridge::open_system_settings("network"),
                                        1 => self.open_vpn_app(),
                                        2 => {
                                            self.modal = ActiveModal::Search;
                                            self.modal_focus = 0;
                                        }
                                        3 => {
                                            self.modal = ActiveModal::Theme;
                                            self.modal_focus = 0;
                                        }
                                        4 => {
                                            self.modal = ActiveModal::A2ui;
                                            self.modal_focus = 0;
                                        }
                                        5 => {
                                            self.spawn_load_apps(ctx.clone());
                                            self.spawn_load_status(ctx.clone());
                                            self.spawn_load_weather(ctx.clone());
                                            self.show_toast("Refreshed apps & live London weather");
                                        }
                                        6 => {
                                            self.modal = ActiveModal::Settings;
                                            self.modal_focus = 0;
                                        }
                                        _ => {}
                                    }
                                }
                                resp.on_hover_text(*tip);
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
        let center = Pos2::new(rect.center().x, rect.top() + icon_d * 0.5 + 8.0 - focus_t * 2.5);
        let radius = icon_d * (0.48 + focus_t * 0.055);

        // Drop shadow
        p.circle_filled(
            center + Vec2::new(0.0, 4.0 + focus_t * 2.0),
            radius + 1.0,
            Color32::from_rgba_unmultiplied(0, 0, 0, (90.0 + focus_t * 55.0) as u8),
        );

        // Smooth animated focus glow halo + active focus animation effect
        if focus_t > 0.01 {
            p.circle_filled(
                center,
                radius + 6.5 * focus_t,
                Color32::from_rgba_unmultiplied(
                    accent.r(),
                    accent.g(),
                    accent.b(),
                    (focus_t * 72.0) as u8,
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

        // Crisp circular glass rim
        p.circle_stroke(
            center,
            radius,
            if focused {
                Stroke::new(2.8, accent)
            } else {
                Stroke::new(1.2, Color32::from_rgba_unmultiplied(255, 255, 255, 55))
            },
        );

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
        let pop_scale = if self.config.modal_pop_in {
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
                                if ui
                                    .add(
                                        egui::Button::new(
                                            RichText::new("✕ Close")
                                                .font(FontId::proportional(13.0))
                                                .strong()
                                                .color(Color32::WHITE),
                                        )
                                        .fill(Color32::from_rgba_unmultiplied(255, 255, 255, 22))
                                        .stroke(Stroke::new(1.0, accent))
                                        .corner_radius(CornerRadius::same(10)),
                                    )
                                    .clicked()
                                {
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
        let allow_keys = !self.modal_just_opened && !self.modal_wait_key_release;
        self.modal_just_opened = false;

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
                            }
                            1 => {
                                if left { self.modal_focus = 0; }
                                else if down { self.modal_focus = 3; }
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
                                    RichText::new("Your launcher is pre-configured with Zenith Horizon, 3×3 Grid Settings, and Live 1km UK Satellite. Customize below or press OK to start.")
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
                                    ctx.request_repaint_after(Duration::from_millis(24));
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
                            } else if up && self.modal_focus >= THEME_COLS {
                                self.modal_focus -= THEME_COLS;
                                self.modal_focus_changed = true;
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
                                self.modal_focus = self.modal_focus.saturating_sub(1);
                                self.modal_focus_changed = true;
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

                                        // Top-left circular vector icon badge
                                        let ic_c = Pos2::new(rect.left() + 26.0, rect.top() + 26.0);
                                        p.circle_filled(
                                            ic_c,
                                            13.5,
                                            if focused {
                                                Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 85)
                                            } else {
                                                Color32::from_rgba_unmultiplied(255, 255, 255, 18)
                                            },
                                        );
                                        Self::draw_theme_icon(
                                            p,
                                            ic_c,
                                            idx,
                                            if focused { Color32::WHITE } else { Color32::from_rgb(205, 220, 242) },
                                            accent,
                                        );

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
                                        ui.painter().circle_filled(
                                            ic_r.center(),
                                            11.5,
                                            if focused {
                                                Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 65)
                                            } else {
                                                Color32::from_rgba_unmultiplied(255, 255, 255, 16)
                                            },
                                        );
                                        Self::draw_theme_icon(
                                            ui.painter(),
                                            ic_r.center(),
                                            idx,
                                            if focused {
                                                Color32::WHITE
                                            } else {
                                                Color32::from_rgb(205, 218, 240)
                                            },
                                            accent,
                                        );
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
                // 15 settings (0..15, perfect 5x3 grid!) + row 15 = Android TV Shortcuts (4 columns)
                const SETTING_COUNT: usize = 15;
                const SYS_IDX: usize = 15;
                const GRID_COLS: usize = 3;
                let is_grid = self.config.settings_view == SettingsViewMode::Grid;

                let mut trigger = false;
                let mut left_key = false;
                let mut right_key = false;
                if allow_keys {
                    ctx.input(|i| {
                        let up = i.key_pressed(Key::ArrowUp);
                        let down = i.key_pressed(Key::ArrowDown);
                        let left = i.key_pressed(Key::ArrowLeft);
                        let right = i.key_pressed(Key::ArrowRight);
                        let ok = i.key_pressed(Key::Enter) || i.key_pressed(Key::Space);

                        if is_grid {
                            if self.modal_focus == SYS_IDX {
                                if left {
                                    self.settings_sys_col = self.settings_sys_col.saturating_sub(1);
                                } else if right {
                                    self.settings_sys_col = (self.settings_sys_col + 1).min(3);
                                } else if up {
                                    self.modal_focus = SETTING_COUNT - 1;
                                    self.modal_focus_changed = true;
                                } else if ok {
                                    trigger = true;
                                }
                            } else {
                                let col = self.modal_focus % GRID_COLS;
                                if left && col > 0 {
                                    self.modal_focus -= 1;
                                    self.modal_focus_changed = true;
                                } else if right && col + 1 < GRID_COLS && self.modal_focus + 1 < SETTING_COUNT {
                                    self.modal_focus += 1;
                                    self.modal_focus_changed = true;
                                } else if up && self.modal_focus >= GRID_COLS {
                                    self.modal_focus -= GRID_COLS;
                                    self.modal_focus_changed = true;
                                } else if down {
                                    if self.modal_focus + GRID_COLS < SETTING_COUNT {
                                        self.modal_focus += GRID_COLS;
                                    } else if self.modal_focus / GRID_COLS < (SETTING_COUNT - 1) / GRID_COLS {
                                        self.modal_focus = SETTING_COUNT - 1;
                                    } else {
                                        self.modal_focus = SYS_IDX;
                                    }
                                    self.modal_focus_changed = true;
                                } else if ok {
                                    trigger = true;
                                }
                            }
                        } else {
                            if down {
                                self.modal_focus = (self.modal_focus + 1).min(SYS_IDX);
                                self.modal_focus_changed = true;
                            } else if up {
                                self.modal_focus = self.modal_focus.saturating_sub(1);
                                self.modal_focus_changed = true;
                            } else if left {
                                left_key = true;
                            } else if right {
                                right_key = true;
                            } else if ok {
                                trigger = true;
                            }
                        }
                    });
                }

                let apply_setting = |this: &mut Self, idx: usize| match idx {
                    // Category 1: LAYOUT & DISPLAY (0..6)
                    0 => {
                        this.config.layout_style = match this.config.layout_style {
                            LayoutStyle::Zenith => LayoutStyle::Classic,
                            LayoutStyle::Classic => LayoutStyle::Zenith,
                        };
                        this.save_config();
                        this.show_toast(format!("Switched to {}", this.config.layout_style.label()));
                    }
                    1 => {
                        this.config.settings_view = match this.config.settings_view {
                            SettingsViewMode::Grid => SettingsViewMode::List,
                            SettingsViewMode::List => SettingsViewMode::Grid,
                        };
                        this.save_config();
                    }
                    2 => {
                        this.config.icon_size = match this.config.icon_size {
                            IconSize::Compact => IconSize::Standard,
                            IconSize::Standard => IconSize::Large,
                            IconSize::Large => IconSize::Compact,
                        };
                        this.save_config();
                    }
                    3 => {
                        this.config.show_header = !this.config.show_header;
                        this.save_config();
                    }
                    4 => {
                        this.config.show_labels = !this.config.show_labels;
                        this.save_config();
                    }
                    5 => {
                        this.config.auto_night_dim = !this.config.auto_night_dim;
                        this.save_config();
                    }
                    // Category 2: WEATHER & STATUS (6..9)
                    6 => {
                        this.config.show_weather = !this.config.show_weather;
                        this.save_config();
                    }
                    7 => {
                        this.config.clock_24h = !this.config.clock_24h;
                        this.save_config();
                    }
                    8 => {
                        this.config.show_status_pill = !this.config.show_status_pill;
                        this.save_config();
                    }
                    // Category 3: APPS & LIBRARY (9..12)
                    9 => {
                        this.config.sort_order = match this.config.sort_order {
                            SortOrder::Alpha => SortOrder::Recent,
                            SortOrder::Recent => SortOrder::Alpha,
                        };
                        this.save_config();
                    }
                    10 => {
                        this.config.show_mobile_apps = !this.config.show_mobile_apps;
                        this.save_config();
                    }
                    11 => {
                        this.config.hidden_apps.clear();
                        this.save_config();
                        this.show_toast("Restored all hidden apps");
                    }
                    // Category 4: SYSTEM & UPDATES (12..15)
                    12 => {
                        this.config.show_system_apps = !this.config.show_system_apps;
                        this.save_config();
                    }
                    13 => {
                        let ok = bridge::open_home_settings();
                        if !ok {
                            this.show_toast("Open Android Settings → Apps → Default Home");
                        }
                    }
                    14 => {
                        this.open_modal(ActiveModal::Updater);
                        if matches!(
                            this.updater_state,
                            UpdaterState::Idle | UpdaterState::Error(_)
                        ) {
                            this.spawn_check_update(ctx.clone(), false);
                        }
                    }
                    _ => {}
                };

                if self.modal_focus == SYS_IDX {
                    if !is_grid {
                        if left_key {
                            self.settings_sys_col = self.settings_sys_col.saturating_sub(1);
                        } else if right_key {
                            self.settings_sys_col = (self.settings_sys_col + 1).min(3);
                        }
                    }
                    if trigger {
                        let sec = match self.settings_sys_col {
                            0 => "network",
                            1 => "apps",
                            2 => "display",
                            _ => "main",
                        };
                        bridge::open_system_settings(sec);
                    }
                } else if trigger || (!is_grid && (left_key || right_key)) {
                    apply_setting(self, self.modal_focus);
                }

                let modal_title = if is_grid {
                    "Launcher Settings  ·  D-Pad navigate · OK toggle"
                } else {
                    "Launcher Settings  ·  Up/Down move · OK toggle"
                };

                self.draw_modal_shell(
                    ctx,
                    modal_title,
                    820.0,
                    465.0,
                    |this, ui, _ctx| {
                        let accent = this.config.accent();

                        // 3-Card Live Telemetry Bar (Device · RAM · Storage) with crisp vector icons
                        ui.columns(3, |cols| {
                            let card = |ui: &mut Ui, icon_id: usize, title: &str, val: String, ratio: Option<f32>, bar_c: Color32| {
                                egui::Frame::NONE
                                    .fill(Color32::from_rgba_unmultiplied(255, 255, 255, 13))
                                    .stroke(Stroke::new(
                                        1.0_f32,
                                        Color32::from_rgba_unmultiplied(255, 255, 255, 24),
                                    ))
                                    .corner_radius(CornerRadius::same(12))
                                    .inner_margin(egui::Margin::symmetric(12, 8))
                                    .show(ui, |ui| {
                                        ui.set_min_width(ui.available_width());
                                        ui.horizontal(|ui| {
                                            let (ir, _) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::hover());
                                            ui.painter().circle_filled(
                                                ir.center(),
                                                11.0,
                                                Color32::from_rgba_unmultiplied(bar_c.r(), bar_c.g(), bar_c.b(), 36),
                                            );
                                            Self::draw_setting_icon(ui.painter(), ir.center(), icon_id, bar_c);
                                            ui.vertical(|ui| {
                                                ui.label(
                                                    RichText::new(title)
                                                        .font(FontId::proportional(9.5))
                                                        .strong()
                                                        .color(Color32::from_rgb(165, 182, 210)),
                                                );
                                                ui.label(
                                                    RichText::new(val)
                                                        .font(FontId::proportional(12.0))
                                                        .strong()
                                                        .color(Color32::WHITE),
                                                );
                                            });
                                        });
                                        if let Some(r) = ratio {
                                            ui.add_space(4.0);
                                            let (bar_rect, _) = ui.allocate_exact_size(
                                                Vec2::new(ui.available_width(), 4.0),
                                                Sense::hover(),
                                            );
                                            let p = ui.painter();
                                            p.rect_filled(
                                                bar_rect,
                                                CornerRadius::same(2),
                                                Color32::from_rgba_unmultiplied(255, 255, 255, 24),
                                            );
                                            let mut fill_r = bar_rect;
                                            fill_r.set_width((bar_rect.width() * r.clamp(0.05, 1.0)).max(4.0));
                                            p.rect_filled(fill_r, CornerRadius::same(2), bar_c);
                                        }
                                    });
                            };

                            card(
                                &mut cols[0],
                                100,
                                "DEVICE",
                                format!("{} · Android {}", this.status.device_model, this.status.android_version),
                                None,
                                accent,
                            );
                            let ram_used = (this.status.ram_total_mb - this.status.ram_free_mb).max(0) as f32
                                / (this.status.ram_total_mb.max(1) as f32);
                            card(
                                &mut cols[1],
                                101,
                                "MEMORY (RAM)",
                                format!("{} / {} MB free", this.status.ram_free_mb, this.status.ram_total_mb),
                                Some(ram_used),
                                accent,
                            );
                            let sto_used = ((this.status.storage_total_gb - this.status.storage_free_gb).max(0.0)
                                / this.status.storage_total_gb.max(0.1)) as f32;
                            card(
                                &mut cols[2],
                                102,
                                "STORAGE",
                                format!("{:.1} / {:.1} GB free", this.status.storage_free_gb, this.status.storage_total_gb),
                                Some(sto_used),
                                Color32::from_rgb(52, 211, 153),
                            );
                        });
                        ui.add_space(10.0);

                        // (label, value, is_on, icon_id) — grouped in exact multiples of 3!
                        let rows: [(&str, String, bool, usize); SETTING_COUNT] = [
                            // LAYOUT & DISPLAY (0..6)
                            ("Layout Style", this.config.layout_style.label().to_string(), true, 0),
                            ("Settings View", this.config.settings_view.label().to_string(), true, 201),
                            ("Icon Size", this.config.icon_size.label().to_string(), true, 1),
                            (
                                "Clock & Weather Header",
                                if this.config.show_header { "ON" } else { "OFF" }.to_string(),
                                this.config.show_header,
                                3,
                            ),
                            (
                                "Show App Labels",
                                if this.config.show_labels { "ON" } else { "OFF" }.to_string(),
                                this.config.show_labels,
                                5,
                            ),
                            (
                                "Auto Night Dimming",
                                if this.config.auto_night_dim { "ON" } else { "OFF" }.to_string(),
                                this.config.auto_night_dim,
                                8,
                            ),
                            // WEATHER & STATUS (6..9)
                            (
                                "Live London Weather",
                                if this.config.show_weather { "ON" } else { "OFF" }.to_string(),
                                this.config.show_weather,
                                4,
                            ),
                            (
                                "24-Hour UK Clock",
                                if this.config.clock_24h { "24h" } else { "12h" }.to_string(),
                                this.config.clock_24h,
                                6,
                            ),
                            (
                                "Live Status Pill",
                                if this.config.show_status_pill { "ON" } else { "OFF" }.to_string(),
                                this.config.show_status_pill,
                                7,
                            ),
                            // APPS & LIBRARY (9..12)
                            (
                                "App Sort Order",
                                match this.config.sort_order {
                                    SortOrder::Alpha => "A–Z".to_string(),
                                    SortOrder::Recent => "Recent".to_string(),
                                },
                                true,
                                2,
                            ),
                            (
                                "Mobile / Sideloaded",
                                if this.config.show_mobile_apps { "ON" } else { "OFF" }.to_string(),
                                this.config.show_mobile_apps,
                                9,
                            ),
                            (
                                "Hidden Apps",
                                format!("Unhide ({})", this.config.hidden_apps.len()),
                                false,
                                12,
                            ),
                            // SYSTEM & UPDATES (12..15)
                            (
                                "System Apps",
                                if this.config.show_system_apps { "ON" } else { "OFF" }.to_string(),
                                this.config.show_system_apps,
                                10,
                            ),
                            (
                                "Default Home",
                                if this.is_default_home {
                                    "✓ Default".to_string()
                                } else {
                                    "Set Default ›".to_string()
                                },
                                this.is_default_home,
                                11,
                            ),
                            (
                                "App Updates",
                                match &this.updater_state {
                                    UpdaterState::Available(r) => format!("v{} ›", r.version),
                                    UpdaterState::Downloading { .. } => "Downloading".to_string(),
                                    UpdaterState::ReadyToInstall { .. }
                                    | UpdaterState::NeedPermission { .. } => "Install ›".to_string(),
                                    UpdaterState::Checking => "Checking...".to_string(),
                                    _ => format!("v{} ›", this.app_version),
                                },
                                this.has_update_available(),
                                13,
                            ),
                        ];

                        let category_for = |idx: usize| -> Option<&'static str> {
                            match idx {
                                0 => Some("LAYOUT & DISPLAY"),
                                6 => Some("WEATHER & STATUS"),
                                9 => Some("APPS & LIBRARY"),
                                12 => Some("SYSTEM & UPDATES"),
                                _ => None,
                            }
                        };

                        let draw_category_header = |ui: &mut Ui, title: &str, top_gap: bool| {
                            if top_gap {
                                ui.add_space(6.0);
                            }
                            ui.horizontal(|ui| {
                                let (bar_r, _) =
                                    ui.allocate_exact_size(Vec2::new(4.0, 13.0), Sense::hover());
                                ui.painter().rect_filled(bar_r, CornerRadius::same(2), accent);
                                ui.label(
                                    RichText::new(title)
                                        .font(FontId::proportional(10.5))
                                        .strong()
                                        .color(accent),
                                );
                                let avail = ui.available_width();
                                if avail > 12.0 {
                                    let (lr, _) =
                                        ui.allocate_exact_size(Vec2::new(avail, 13.0), Sense::hover());
                                    let y = lr.center().y;
                                    ui.painter().line_segment(
                                        [Pos2::new(lr.left() + 4.0, y), Pos2::new(lr.right(), y)],
                                        Stroke::new(
                                            1.0_f32,
                                            Color32::from_rgba_unmultiplied(255, 255, 255, 22),
                                        ),
                                    );
                                }
                            });
                            ui.add_space(4.0);
                        };

                        let mut clicked_idx: Option<usize> = None;

                        if this.config.settings_view == SettingsViewMode::Grid {
                            // 3-Column Categorized Glass Card Grid View
                            for (row_i, chunk) in rows.chunks(GRID_COLS).enumerate() {
                                let first_idx = row_i * GRID_COLS;
                                if let Some(cat) = category_for(first_idx) {
                                    draw_category_header(ui, cat, first_idx > 0);
                                }
                                ui.columns(GRID_COLS, |cols| {
                                    for (c_i, (label, val, is_on, icon_id)) in chunk.iter().enumerate() {
                                        let idx = row_i * GRID_COLS + c_i;
                                        let focused = this.modal_focus == idx;
                                        let w = cols[c_i].available_width();
                                        let (rect, resp) = cols[c_i]
                                            .allocate_exact_size(Vec2::new(w, 60.0), Sense::click());
                                        let p = cols[c_i].painter();

                                        p.rect_filled(
                                            rect,
                                            CornerRadius::same(12),
                                            if focused {
                                                Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 48)
                                            } else {
                                                Color32::from_rgba_unmultiplied(255, 255, 255, 11)
                                            },
                                        );
                                        p.rect_stroke(
                                            rect,
                                            CornerRadius::same(12),
                                            if focused {
                                                Stroke::new(2.0_f32, accent)
                                            } else {
                                                Stroke::new(
                                                    1.0_f32,
                                                    Color32::from_rgba_unmultiplied(255, 255, 255, 20),
                                                )
                                            },
                                            egui::StrokeKind::Inside,
                                        );

                                        // Top-left circular icon badge
                                        let ic_c = Pos2::new(rect.left() + 22.0, rect.top() + 20.0);
                                        p.circle_filled(
                                            ic_c,
                                            11.5,
                                            if focused {
                                                Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 85)
                                            } else {
                                                Color32::from_rgba_unmultiplied(255, 255, 255, 18)
                                            },
                                        );
                                        Self::draw_setting_icon(
                                            p,
                                            ic_c,
                                            *icon_id,
                                            if focused { Color32::WHITE } else { Color32::from_rgb(205, 220, 242) },
                                        );

                                        // Top-right status pill badge
                                        let pill_w = (val.len() as f32 * 6.8 + 18.0).clamp(48.0, 108.0);
                                        let pill_rect = Rect::from_min_size(
                                            Pos2::new(rect.right() - pill_w - 10.0, rect.top() + 10.0),
                                            Vec2::new(pill_w, 20.0),
                                        );
                                        p.rect_filled(
                                            pill_rect,
                                            CornerRadius::same(7),
                                            if focused {
                                                Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 115)
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
                                            FontId::proportional(11.0),
                                            Color32::WHITE,
                                        );

                                        // Bottom-left bold setting label
                                        p.text(
                                            Pos2::new(rect.left() + 12.0, rect.bottom() - 14.0),
                                            Align2::LEFT_CENTER,
                                            *label,
                                            FontId::proportional(12.5),
                                            if focused { Color32::WHITE } else { Color32::from_rgb(225, 233, 246) },
                                        );

                                        if focused && this.modal_focus_changed {
                                            resp.scroll_to_me(Some(Align::Center));
                                            this.modal_focus_changed = false;
                                        }
                                        if resp.clicked() {
                                            this.modal_focus = idx;
                                            clicked_idx = Some(idx);
                                        }
                                    }
                                });
                                ui.add_space(6.0);
                            }
                        } else {
                            // Classic Categorized List View
                            for (idx, (label, val, is_on, icon_id)) in rows.iter().enumerate() {
                                if let Some(sec) = category_for(idx) {
                                    draw_category_header(ui, sec, idx > 0);
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
                                    .inner_margin(egui::Margin::symmetric(12, 5))
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            let (dot_r, _) = ui.allocate_exact_size(Vec2::new(5.0, 22.0), Sense::hover());
                                            if focused {
                                                ui.painter().rect_filled(
                                                    Rect::from_center_size(dot_r.center(), Vec2::new(3.5, 18.0)),
                                                    CornerRadius::same(2),
                                                    accent,
                                                );
                                            }
                                            let (ic_r, _) = ui.allocate_exact_size(Vec2::splat(24.0), Sense::hover());
                                            ui.painter().circle_filled(
                                                ic_r.center(),
                                                11.5,
                                                if focused {
                                                    Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 65)
                                                } else {
                                                    Color32::from_rgba_unmultiplied(255, 255, 255, 16)
                                                },
                                            );
                                            Self::draw_setting_icon(
                                                ui.painter(),
                                                ic_r.center(),
                                                *icon_id,
                                                if focused {
                                                    Color32::WHITE
                                                } else {
                                                    Color32::from_rgb(205, 218, 240)
                                                },
                                            );
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
                                                        .min_size(Vec2::new(98.0, 26.0)),
                                                    )
                                                    .clicked()
                                                {
                                                    this.modal_focus = idx;
                                                    clicked_idx = Some(idx);
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

                        if let Some(ci) = clicked_idx {
                            apply_setting(this, ci);
                        }

                        ui.add_space(6.0);
                        ui.label(
                            RichText::new("ANDROID TV SHORTCUTS")
                                .font(FontId::proportional(10.5))
                                .strong()
                                .color(accent),
                        );
                        ui.add_space(4.0);
                        let sys_row = this.modal_focus == SYS_IDX;
                        let items = [
                            ("Network", "network"),
                            ("Apps", "apps"),
                            ("Display & Sound", "display"),
                            ("System Settings", "main"),
                        ];
                        ui.columns(4, |cols| {
                            for (c, (lbl, sec)) in items.iter().enumerate() {
                                let w = cols[c].available_width();
                                let f = sys_row && this.settings_sys_col == c;
                                let (rect, resp) = cols[c].allocate_exact_size(Vec2::new(w, 36.0), Sense::click());
                                let p = cols[c].painter();
                                p.rect_filled(
                                    rect,
                                    CornerRadius::same(10),
                                    if f {
                                        Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 85)
                                    } else {
                                        Color32::from_rgba_unmultiplied(255, 255, 255, 16)
                                    },
                                );
                                p.rect_stroke(
                                    rect,
                                    CornerRadius::same(10),
                                    if f {
                                        Stroke::new(1.8_f32, accent)
                                    } else {
                                        Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 28))
                                    },
                                    egui::StrokeKind::Inside,
                                );
                                let ic_center = Pos2::new(rect.left() + 20.0, rect.center().y);
                                Self::draw_setting_icon(p, ic_center, 200 + c, Color32::WHITE);
                                p.text(
                                    Pos2::new(rect.left() + 34.0, rect.center().y),
                                    Align2::LEFT_CENTER,
                                    *lbl,
                                    FontId::proportional(12.0),
                                    Color32::WHITE,
                                );
                                if f && this.modal_focus_changed {
                                    resp.scroll_to_me(Some(Align::Center));
                                    this.modal_focus_changed = false;
                                }
                                if resp.clicked() {
                                    this.modal_focus = SYS_IDX;
                                    this.settings_sys_col = c;
                                    bridge::open_system_settings(sec);
                                }
                            }
                        });
                    },
                );
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

            ActiveModal::A2ui => {
                let mut trigger_btn = false;
                if allow_keys {
                    ctx.input(|i| {
                        if i.key_pressed(Key::ArrowLeft) {
                            self.modal_focus = self.modal_focus.saturating_sub(1);
                        } else if i.key_pressed(Key::ArrowRight) {
                            self.modal_focus = (self.modal_focus + 1).min(2);
                        } else if i.key_pressed(Key::Enter) || i.key_pressed(Key::Space) {
                            trigger_btn = true;
                        }
                    });
                }
                if trigger_btn {
                    match self.modal_focus {
                        0 => match self.a2ui.process_json(&self.a2ui_input) {
                            Ok(n) => {
                                self.a2ui_feedback =
                                    format!("✓ Applied {n} A2UI v1.0 message(s) natively in Rust");
                            }
                            Err(e) => {
                                self.a2ui_feedback = format!("Error: {e}");
                            }
                        },
                        1 => {
                            self.a2ui_input = SAMPLE_A2UI_PAYLOAD.to_string();
                            self.a2ui_feedback = "Loaded sample A2UI v1.0 payload".to_string();
                        }
                        _ => {
                            self.a2ui.clear_custom();
                            self.a2ui_feedback = "Cleared custom surfaces".to_string();
                        }
                    }
                }

                self.draw_modal_shell(
                    ctx,
                    "A2UI v1.0 Native Surface Studio  (Left/Right select · OK run)",
                    760.0,
                    430.0,
                    |this, ui, _ctx| {
                        let accent = this.config.accent();
                        ui.horizontal(|ui| {
                            if Self::modal_btn(ui, "Inject Live Surface", true, this.modal_focus == 0, accent) {
                                this.modal_focus = 0;
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
                            if Self::modal_btn(ui, "Load Sample", false, this.modal_focus == 1, accent) {
                                this.modal_focus = 1;
                                this.a2ui_input = SAMPLE_A2UI_PAYLOAD.to_string();
                            }
                            if Self::modal_btn(ui, "Clear Surfaces", false, this.modal_focus == 2, accent) {
                                this.modal_focus = 2;
                                this.a2ui.clear_custom();
                                this.a2ui_feedback = "Cleared custom surfaces".to_string();
                            }
                        });
                        if !this.a2ui_feedback.is_empty() {
                            ui.add_space(6.0);
                            ui.label(RichText::new(&this.a2ui_feedback).color(accent).strong());
                        }
                        ui.add_space(8.0);
                        ui.add(
                            egui::TextEdit::multiline(&mut this.a2ui_input)
                                .desired_rows(12)
                                .desired_width(f32::INFINITY)
                                .code_editor(),
                        );
                    },
                );
            }

            ActiveModal::AppContext(pkg) => {
                let app_label = self
                    .apps
                    .iter()
                    .find(|a| a.package_name == pkg)
                    .map(|a| self.display_label(a))
                    .unwrap_or_else(|| pkg.clone());
                let is_fav = self.config.favorites.contains(&pkg);

                let actions: Vec<(&str, &str)> = vec![
                    ("open", "Open App"),
                    ("pin", if is_fav { "Unpin from Favorites" } else { "Pin to Favorites" }),
                    ("left", "Move App Left"),
                    ("right", "Move App Right"),
                    ("rename", "Rename App"),
                    ("hide", "Hide App"),
                    ("info", "Android App Info"),
                    ("uninstall", "Uninstall App"),
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
                    420.0,
                    430.0,
                    |this, ui, _ctx| {
                        let accent = this.config.accent();
                        ui.label(
                            RichText::new(&pkg)
                                .font(FontId::proportional(11.5))
                                .color(Color32::from_rgb(175, 192, 218)),
                        );
                        ui.add_space(10.0);

                        let full_w = ui.available_width();
                        for (idx, (id, label)) in actions.iter().enumerate() {
                            let focused = this.modal_focus == idx;
                            let fill = if focused {
                                Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 85)
                            } else {
                                Color32::from_rgba_unmultiplied(255, 255, 255, 16)
                            };
                            let stroke = if focused {
                                Stroke::new(2.0_f32, accent)
                            } else {
                                Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 26))
                            };
                            let (rect, resp) = ui.allocate_exact_size(Vec2::new(full_w, 36.0), Sense::click());
                            let p = ui.painter();
                            p.rect_filled(rect, CornerRadius::same(10), fill);
                            p.rect_stroke(rect, CornerRadius::same(10), stroke, egui::StrokeKind::Inside);

                            let ic_c = Pos2::new(rect.left() + 22.0, rect.center().y);
                            let ic_col = if *id == "uninstall" {
                                Color32::from_rgb(248, 113, 113)
                            } else if *id == "pin" {
                                Color32::from_rgb(250, 204, 21)
                            } else {
                                Color32::WHITE
                            };
                            Self::draw_action_icon(p, ic_c, id, ic_col);
                            p.text(
                                Pos2::new(rect.left() + 42.0, rect.center().y),
                                Align2::LEFT_CENTER,
                                *label,
                                FontId::proportional(14.0),
                                Color32::WHITE,
                            );
                            if resp.clicked() {
                                chosen = Some(*id);
                            }
                            ui.add_space(5.0);
                        }
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
                                    let w_txt = if let Some(w) = &self.weather {
                                        format!("{} {} · London ›", self.fmt_temp(w.temp_c), w.condition)
                                    } else {
                                        "London Weather ›".to_string()
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

                                // 7 crisp vector quick-action icons
                                ui.spacing_mut().item_spacing.x = 6.0;
                                let labels = [
                                    "Wi-Fi", "VPN", "Search", "Theme", "A2UI", "Refresh", "Settings",
                                ];
                                for (idx, tip) in labels.iter().enumerate() {
                                    let focused = self.modal == ActiveModal::None
                                        && self.focus == FocusTarget::Dock(idx);
                                    let anim = ctx.animate_bool_with_time(
                                        egui::Id::new(("zenith_dock_focus", idx)),
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
                                    Self::draw_dock_icon(
                                        p,
                                        rect.center(),
                                        idx,
                                        if focused {
                                            Color32::WHITE
                                        } else {
                                            Color32::from_rgb(225, 232, 245)
                                        },
                                    );
                                    if idx == 0 {
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
                                    } else if idx == 1 && self.status.is_vpn_active {
                                        p.circle_filled(
                                            rect.right_bottom() + Vec2::new(-4.5, -4.5),
                                            3.0,
                                            Color32::from_rgb(56, 189, 248),
                                        );
                                    } else if idx == 6 && self.has_update_available() {
                                        p.circle_filled(
                                            rect.right_top() + Vec2::new(-4.0, 4.0),
                                            3.6,
                                            Color32::from_rgb(52, 211, 153),
                                        );
                                    }
                                    if resp.clicked() {
                                        self.focus = FocusTarget::Dock(idx);
                                        match idx {
                                            0 => bridge::open_system_settings("network"),
                                            1 => self.open_vpn_app(),
                                            2 => self.open_modal(ActiveModal::Search),
                                            3 => self.open_modal(ActiveModal::Theme),
                                            4 => self.open_modal(ActiveModal::A2ui),
                                            5 => {
                                                self.spawn_load_apps(ctx.clone());
                                                self.spawn_load_status(ctx.clone());
                                                self.spawn_load_weather(ctx.clone());
                                                self.show_toast("Refreshed apps & live London weather");
                                            }
                                            6 => self.open_modal(ActiveModal::Settings),
                                            _ => {}
                                        }
                                    }
                                    resp.on_hover_text(*tip);
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
                                    let center = Pos2::new(
                                        slot_rect.center().x,
                                        slot_rect.top() + icon_d * 0.5 + 8.0 - t * 4.0 - nt * 2.0,
                                    );
                                    let radius = icon_d * (0.46 + t * 0.065 + nt * 0.03);
                                    let p = ui.painter();

                                    // Active focus animation effect (Ripple / Ambient Glow / Breathing Halo)
                                    self.draw_focus_effects(ctx, p, center, radius, is_focused, accent);

                                    // Drop shadow
                                    p.circle_filled(
                                        center + Vec2::new(0.0, 3.5 + t * 2.0),
                                        radius + 1.0,
                                        Color32::from_rgba_unmultiplied(0, 0, 0, (85.0 + t * 50.0) as u8),
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
}

impl eframe::App for AstryxTvApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_messages(ctx);

        // Periodic background refreshes
        if self.last_status_poll.elapsed() > Duration::from_secs(20) {
            self.spawn_load_status(ctx.clone());
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
                                                    Color32::from_rgb(56, 189, 248)
                                                } else {
                                                    Color32::from_rgb(175, 188, 210)
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

        // Active modal overlay
        self.render_modals(ctx, &visible);

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
