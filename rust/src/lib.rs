mod a2ui;
mod app;
mod bridge;
mod theme;
mod updater;
mod weather;
mod web_server;

use android_activity::AndroidApp;
use app::AstryxTvApp;

#[no_mangle]
fn android_main(app: AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default().with_max_level(log::LevelFilter::Info),
    );
    log::info!("Starting Astryx Rust Launcher...");
    bridge::init(app.vm_as_ptr(), app.activity_as_ptr());

    let mut options = eframe::NativeOptions::default();
    options.android_app = Some(app);

    if let Err(e) = eframe::run_native(
        "Astryx Rust Launcher",
        options,
        Box::new(|cc| Ok(Box::new(AstryxTvApp::new(cc)))),
    ) {
        log::error!("eframe exited with error: {e:?}");
    }
}
