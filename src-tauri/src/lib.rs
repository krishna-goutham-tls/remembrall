pub mod correlation;
pub mod db;
pub mod decay;
pub mod fswatcher;
pub mod parser;
pub mod redaction;

use tauri::{
    menu::{Menu, MenuItem},
    tray::{TrayIcon, TrayIconBuilder},
    Manager, Runtime,
};

pub fn run() {
    // Initialize logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    log::info!("Starting Remembrall application");

    tauri::Builder::default()
        .setup(|app| {
            log::info!("Setting up Remembrall");
            setup_tray(app)?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn setup_tray<R: Runtime>(app: &tauri::App<R>) -> Result<TrayIcon<R>, Box<dyn std::error::Error>> {
    let quit_item = MenuItem::with_id(app, "quit", "Quit Remembrall", true, None::<&str>)?;
    let show_item = MenuItem::with_id(app, "show", "Show Window", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show_item, &quit_item])?;

    let tray = TrayIconBuilder::new()
        .menu(&menu)
        .tooltip("Remembrall")
        .on_menu_event(|app, event| match event.id.as_ref() {
            "quit" => {
                log::info!("Quit requested from tray");
                app.exit(0);
            }
            "show" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            _ => {}
        })
        .build(app)?;

    log::info!("Tray icon created successfully");
    Ok(tray)
}
