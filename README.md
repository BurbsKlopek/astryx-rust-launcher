# Astryx Rust Launcher

A **Pure Native Rust** Android TV launcher powered by OpenGL ES (`eframe` / `egui` / `glow` + `android-activity`) with zero WebView overhead, built for 4K 60fps TVs.

## Highlights
- **Pure Native Rust OpenGL ES UI:** ~64 MB total RAM footprint, 60fps GPU rendering, custom `highp` UV / `mediump` FP16 Mali-optimized GLES shaders, and native Android TV `Google Sans` typography.
- **Two Switchable Home Layouts:**
  - **Zenith Horizon** (default): Unified frosted top-bar capsule (realtime UK clock + live London weather + quick-action icons) above an open sky horizon and floating liquid-glass bottom dock with a shared-element sliding focus halo.
  - **Classic Grid:** Borderless bold UK clock & weather header with circular app grid and pinned favorites row.
- **Live 1km Satellite & Weather Wallpapers:**
  - **Live UK Sat:** EUMETSAT Meteosat-12 Third Generation (MTG-I1) 1km GeoColor live satellite over London & Europe (daytime clouds + glowing city lights at night, auto-refreshed every 10 minutes).
  - **Live Earth:** NOAA GOES-19 full-disk Earth from space with twinkling stars and shooting stars.
  - **Auto (Live London Weather):** Matches Open-Meteo London conditions (`Clear Day`, `Golden Hour`, `Starry Night`, `Cloudy`, `Fog`, `Rain`, `Storm`, `Snow`) with real-time sky drift and GPU weather particles.
- **5 Switchable 60fps Focus Animations:** `Kinetic Ripple`, `Magnetic Wave`, `Ambient Glow`, `Breathing Halo`, and `Minimal` — plus live animated preview on the First Launch Welcome screen and 85ms Modal Pop-In transitions.
- **3×3 Glass Card Grid Settings & Theme Customiser:** Toggleable between `Grid Cards` and `Compact List`, with crisp GPU vector icons on every option and direct `OK` to change.
- **TV Remote & Power Features:** Native `AInputQueue` `BACK` & `MENU` support, **Hold OK** App Options (Pin, Move Left/Right, Rename, Hide, App Info, Uninstall), interactive **Clear RAM** pill, live Network/VPN/RAM telemetry, Auto Night Dimming, and a pure-Rust **A2UI v1.0** surface engine.

## Structure
- `rust/` — Pure Native Rust OpenGL ES launcher (`src/app.rs`, `src/theme.rs`, `src/weather.rs`, `src/a2ui.rs`, `src/bridge.rs`) + patched `vendor/egui_glow` (highp UVs) & `vendor/egui-winit` (Android Back/Menu keys).
- `android-rust/` — Minimal `NativeActivity` Gradle host (`RustLauncherActivity.kt`) for `armeabi-v7a` and `arm64-v8a`.
- `web/` & `android/` — Original WebView prototype (kept for reference).
