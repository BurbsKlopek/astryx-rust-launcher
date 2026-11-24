use egui::Color32;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AstryxThemeId {
    Gothic,
    Neutral,
    Y2k,
    Matcha,
}

impl AstryxThemeId {
    pub const ALL: [Self; 4] = [Self::Gothic, Self::Neutral, Self::Y2k, Self::Matcha];

    pub fn label(self) -> &'static str {
        match self {
            Self::Gothic => "Astryx Gothic",
            Self::Neutral => "Astryx Neutral",
            Self::Y2k => "Astryx Y2K",
            Self::Matcha => "Astryx Matcha",
        }
    }

    pub fn surface_tint(self) -> [u8; 3] {
        match self {
            Self::Gothic => [14, 16, 28],
            Self::Neutral => [18, 20, 26],
            Self::Y2k => [14, 22, 38],
            Self::Matcha => [14, 24, 20],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WeatherScene {
    Auto,
    ClearDay,
    Golden,
    ClearNight,
    Cloudy,
    Fog,
    Rain,
    Storm,
    Snow,
    LiveUkSat,
    LiveEarth,
    AuroraBorealis,
    LiquidMesh,
    DeepSpace,
    CustomLocal,
}

impl WeatherScene {
    pub const ALL: [Self; 15] = [
        Self::Auto,
        Self::ClearDay,
        Self::Golden,
        Self::ClearNight,
        Self::Cloudy,
        Self::Fog,
        Self::Rain,
        Self::Storm,
        Self::Snow,
        Self::LiveUkSat,
        Self::LiveEarth,
        Self::AuroraBorealis,
        Self::LiquidMesh,
        Self::DeepSpace,
        Self::CustomLocal,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "Auto (Live)",
            Self::ClearDay => "Clear Day",
            Self::Golden => "Golden Hour",
            Self::ClearNight => "Starry Night",
            Self::Cloudy => "Cloudy",
            Self::Fog => "Fog",
            Self::Rain => "Rain",
            Self::Storm => "Storm",
            Self::Snow => "Snow",
            Self::LiveUkSat => "Live UK Sat",
            Self::LiveEarth => "Live Earth",
            Self::AuroraBorealis => "Aurora Shader",
            Self::LiquidMesh => "Liquid Mesh",
            Self::DeepSpace => "Deep Space",
            Self::CustomLocal => "Custom 4K Photo",
        }
    }

    pub fn is_procedural_shader(self) -> bool {
        matches!(self, Self::AuroraBorealis | Self::LiquidMesh | Self::DeepSpace)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum StartupMode {
    #[default]
    Home,
    LaunchApp,
    LastUsedApp,
    HdmiInput,
}

impl StartupMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Home => "Stay on Home",
            Self::LaunchApp => "Open Chosen App",
            Self::LastUsedApp => "Resume Last App",
            Self::HdmiInput => "TV / HDMI Input",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Home => Self::LaunchApp,
            Self::LaunchApp => Self::LastUsedApp,
            Self::LastUsedApp => Self::HdmiInput,
            Self::HdmiInput => Self::Home,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppFolder {
    pub name: String,
    pub packages: Vec<String>,
}

pub const WEATHER_CITY_PRESETS: [(&str, f64, f64); 7] = [
    ("London · UK", 51.5074, -0.1278),
    ("Birmingham · UK", 52.4862, -1.8904),
    ("Manchester · UK", 53.4808, -2.2426),
    ("Edinburgh · UK", 55.9533, -3.1883),
    ("Bristol · UK", 51.4545, -2.5879),
    ("New York · US", 40.7128, -74.0060),
    ("Tokyo · JP", 35.6762, 139.6503),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IconSize {
    Compact,
    Standard,
    Large,
}

impl IconSize {
    pub fn px(self) -> f32 {
        match self {
            Self::Compact => 62.0,
            Self::Standard => 74.0,
            Self::Large => 88.0,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Compact => "Compact",
            Self::Standard => "Standard",
            Self::Large => "Large",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SortOrder {
    Alpha,
    Recent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum LayoutStyle {
    #[default]
    Zenith,
    Classic,
}

impl LayoutStyle {
    pub fn label(self) -> &'static str {
        match self {
            Self::Zenith => "Zenith Horizon",
            Self::Classic => "Classic Grid",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SettingsViewMode {
    #[default]
    Grid,
    List,
}

impl SettingsViewMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Grid => "Grid Cards",
            Self::List => "Compact List",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum FocusAnimStyle {
    #[default]
    KineticRipple,
    MagneticWave,
    AmbientGlow,
    BreathingHalo,
    Minimal,
}

impl FocusAnimStyle {
    pub const ALL: [Self; 5] = [
        Self::KineticRipple,
        Self::MagneticWave,
        Self::AmbientGlow,
        Self::BreathingHalo,
        Self::Minimal,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::KineticRipple => "Kinetic Ripple",
            Self::MagneticWave => "Magnetic Wave",
            Self::AmbientGlow => "Ambient Glow",
            Self::BreathingHalo => "Breathing Halo",
            Self::Minimal => "Minimal",
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_city() -> String {
    "London · UK".to_string()
}

fn default_lat() -> f64 {
    51.5074
}

fn default_lon() -> f64 {
    -0.1278
}

fn default_ambient() -> u32 {
    180
}

fn default_startup_delay() -> u8 {
    2
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LauncherConfig {
    pub astryx_theme: AstryxThemeId,
    pub weather_scene: WeatherScene,
    pub accent_idx: usize,
    pub menu_transparency: bool,
    pub menu_opacity: f32, // 0.35 .. 0.96
    #[serde(default)]
    pub bg_blur: u8, // 0 .. 5
    #[serde(default)]
    pub layout_style: LayoutStyle,
    #[serde(default)]
    pub settings_view: SettingsViewMode,
    #[serde(default)]
    pub focus_anim: FocusAnimStyle,
    #[serde(default = "default_true")]
    pub modal_pop_in: bool,
    #[serde(default = "default_true")]
    pub live_sky_motion: bool,
    #[serde(default = "default_true")]
    pub show_status_pill: bool,
    #[serde(default = "default_true")]
    pub auto_night_dim: bool,
    #[serde(default)]
    pub onboarding_done: bool,
    #[serde(default)]
    pub show_fps_meter: bool,
    #[serde(default = "default_true")]
    pub overlay_show_fps: bool,
    #[serde(default = "default_true")]
    pub overlay_show_hz: bool,
    #[serde(default = "default_true")]
    pub overlay_show_net: bool,
    #[serde(default)]
    pub overlay_show_ram: bool,
    #[serde(default = "default_true")]
    pub overlay_show_cpu: bool,
    #[serde(default = "default_true")]
    pub overlay_show_gpu: bool,
    #[serde(default = "default_city")]
    pub weather_city: String,
    #[serde(default = "default_lat")]
    pub weather_lat: f64,
    #[serde(default = "default_lon")]
    pub weather_lon: f64,
    #[serde(default = "default_ambient")]
    pub ambient_timeout_secs: u32,
    #[serde(default)]
    pub auto_clear_ram_on_home: bool,
    #[serde(default = "default_true")]
    pub show_now_playing: bool,
    #[serde(default = "default_true")]
    pub web_companion_enabled: bool,
    #[serde(default)]
    pub custom_wallpaper_path: String,
    #[serde(default)]
    pub app_folders: Vec<AppFolder>,
    #[serde(default)]
    pub startup_mode: StartupMode,
    #[serde(default)]
    pub startup_app_pkg: String,
    #[serde(default = "default_startup_delay")]
    pub startup_delay_secs: u8,
    #[serde(default)]
    pub lightweight_mode: bool,
    pub icon_size: IconSize,
    pub sort_order: SortOrder,
    pub show_header: bool,
    pub show_weather: bool,
    pub show_labels: bool,
    pub clock_24h: bool,
    pub show_mobile_apps: bool,
    pub show_system_apps: bool,
    pub favorites: Vec<String>,
    pub hidden_apps: Vec<String>,
    pub recents: Vec<String>,
    #[serde(default)]
    pub custom_order: Vec<String>,
    #[serde(default)]
    pub custom_labels: HashMap<String, String>,
}

impl Default for LauncherConfig {
    fn default() -> Self {
        Self {
            astryx_theme: AstryxThemeId::Gothic,
            weather_scene: WeatherScene::LiveUkSat,
            accent_idx: 0,
            menu_transparency: true,
            menu_opacity: 0.72,
            bg_blur: 0,
            layout_style: LayoutStyle::Zenith,
            settings_view: SettingsViewMode::Grid,
            focus_anim: FocusAnimStyle::KineticRipple,
            modal_pop_in: true,
            live_sky_motion: true,
            show_status_pill: true,
            auto_night_dim: true,
            onboarding_done: false,
            show_fps_meter: false,
            overlay_show_fps: true,
            overlay_show_hz: true,
            overlay_show_net: true,
            overlay_show_ram: false,
            overlay_show_cpu: true,
            overlay_show_gpu: true,
            weather_city: default_city(),
            weather_lat: default_lat(),
            weather_lon: default_lon(),
            ambient_timeout_secs: default_ambient(),
            auto_clear_ram_on_home: false,
            show_now_playing: true,
            web_companion_enabled: true,
            custom_wallpaper_path: String::new(),
            app_folders: Vec::new(),
            startup_mode: StartupMode::Home,
            startup_app_pkg: String::new(),
            startup_delay_secs: default_startup_delay(),
            lightweight_mode: false,
            icon_size: IconSize::Standard,
            sort_order: SortOrder::Alpha,
            show_header: true,
            show_weather: true,
            show_labels: true,
            clock_24h: true,
            show_mobile_apps: true,
            show_system_apps: false,
            favorites: Vec::new(),
            hidden_apps: Vec::new(),
            recents: Vec::new(),
            custom_order: Vec::new(),
            custom_labels: HashMap::new(),
        }
    }
}

pub const ACCENT_PRESETS: [(&str, [u8; 3], [u8; 3]); 6] = [
    ("Sky Blue", [56, 189, 248], [168, 85, 247]),
    ("Violet", [168, 85, 247], [56, 189, 248]),
    ("Neon Rose", [244, 63, 94], [167, 139, 250]),
    ("Emerald", [16, 185, 129], [45, 212, 191]),
    ("Amber Gold", [245, 158, 11], [251, 113, 133]),
    ("Ice White", [226, 232, 240], [96, 165, 250]),
];

impl LauncherConfig {
    pub fn accent(&self) -> Color32 {
        let (_, rgb, _) = ACCENT_PRESETS[self.accent_idx % ACCENT_PRESETS.len()];
        Color32::from_rgb(rgb[0], rgb[1], rgb[2])
    }

    pub fn accent2(&self) -> Color32 {
        let (_, _, rgb) = ACCENT_PRESETS[self.accent_idx % ACCENT_PRESETS.len()];
        Color32::from_rgb(rgb[0], rgb[1], rgb[2])
    }

    pub fn modal_fill(&self) -> Color32 {
        let [r, g, b] = self.astryx_theme.surface_tint();
        let alpha = if self.menu_transparency {
            (self.menu_opacity.clamp(0.32, 0.96) * 255.0) as u8
        } else {
            246
        };
        Color32::from_rgba_unmultiplied(r, g, b, alpha)
    }

    pub fn dock_fill(&self) -> Color32 {
        let [r, g, b] = self.astryx_theme.surface_tint();
        let alpha = if self.menu_transparency {
            ((self.menu_opacity - 0.14).clamp(0.28, 0.9) * 255.0) as u8
        } else {
            232
        };
        Color32::from_rgba_unmultiplied(r, g, b, alpha)
    }

    pub fn overlay_flags(&self) -> i32 {
        let mut f = 0;
        if self.overlay_show_fps {
            f |= 1;
        }
        if self.overlay_show_hz {
            f |= 2;
        }
        if self.overlay_show_net {
            f |= 4;
        }
        if self.overlay_show_ram {
            f |= 8;
        }
        if self.overlay_show_cpu {
            f |= 16;
        }
        if self.overlay_show_gpu {
            f |= 32;
        }
        f
    }
}
