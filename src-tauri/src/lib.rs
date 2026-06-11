pub mod commands;
pub mod correlation;
pub mod db;
pub mod decay;
pub mod fswatcher;
pub mod parser;
pub mod redaction;

use fswatcher::WatcherState;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{TrayIcon, TrayIconBuilder},
    Manager, Runtime, WebviewUrl, WebviewWindowBuilder,
};

/// Application state managed by Tauri.
pub struct AppState;

pub fn run() {
    // Initialize logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    log::info!("Starting Remembrall application");

    tauri::Builder::default()
        .manage(AppState)
        .manage(WatcherState {
            watcher: std::sync::Mutex::new(None),
        })
        .setup(|app| {
            log::info!("Setting up Remembrall");
            setup_tray(app)?;
            create_app_windows(app)?;
            fswatcher::start_watcher(app);
            setup_decay_sweep();
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::search::search_fts5,
            commands::search::search_semantic,
            commands::memories::get_memory,
            commands::memories::reinforce_memory,
            commands::memories::delete_memory,
            commands::memories::reclassify_memory,
            commands::memories::edit_memory_summary,
            commands::backfill::get_backfill_progress,
            commands::backfill::start_backfill,
            commands::backfill::pause_backfill,
            commands::backfill::resume_backfill,
            commands::settings::get_settings,
            commands::settings::update_setting,
            commands::settings::clear_all_memories,
            commands::settings::export_memories_json,
            commands::settings::get_redaction_log,
            commands::ftue::check_fda_permission,
            commands::ftue::download_model,
            commands::ftue::register_mcp,
            commands::ftue::check_mcp_registered,
            commands::browse::get_memories_page,
            commands::browse::get_filters,
            commands::browse::get_projects,
            commands::panel::get_recent_memories,
            commands::panel::get_tool_status,
            commands::panel::quit_app,
            commands::icon_state::get_icon_state,
            commands::icon_state::clear_new_memories,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Create menubar, browse, settings, and ftue windows programmatically.
/// The "main" window is defined in tauri.conf.json and serves as the hidden tray host.
fn create_app_windows<R: Runtime>(app: &tauri::App<R>) -> Result<(), Box<dyn std::error::Error>> {
    // Menubar panel — the primary UI, shown on startup
    if app.get_webview_window("menubar").is_none() {
        WebviewWindowBuilder::new(app, "menubar", WebviewUrl::App("/".into()))
            .title("Remembrall")
            .inner_size(400.0, 600.0)
            .resizable(true)
            .visible(true)
            .build()?;
        log::info!("Menubar panel window created (400x600)");
    }

    // Browse window — 900x650 (hidden until opened from panel)
    if app.get_webview_window("browse").is_none() {
        WebviewWindowBuilder::new(app, "browse", WebviewUrl::App("/browse".into()))
            .title("Remembrall — Browse")
            .inner_size(900.0, 650.0)
            .resizable(true)
            .visible(false)
            .build()?;
        log::info!("Browse window created (hidden, 900x650)");
    }

    // Settings window — 500x400 (hidden until opened from panel)
    if app.get_webview_window("settings").is_none() {
        WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("/settings".into()))
            .title("Remembrall — Settings")
            .inner_size(500.0, 400.0)
            .resizable(true)
            .visible(false)
            .build()?;
        log::info!("Settings window created (hidden, 500x400)");
    }

    // FTUE window — 500x400 (only shown on first run, when brain.db doesn't exist)
    let is_first_run = !db::schema::get_database_path()
        .map(|p| p.exists())
        .unwrap_or(false);
    if is_first_run {
        if app.get_webview_window("ftue").is_none() {
            WebviewWindowBuilder::new(app, "ftue", WebviewUrl::App("/ftue".into()))
                .title("Remembrall — Welcome")
                .inner_size(500.0, 400.0)
                .resizable(true)
                .visible(true)
                .build()?;
            log::info!("FTUE window created — first run detected (brain.db not found)");
        }
    } else {
        log::info!("Skipping FTUE — brain.db exists, not first run");
    }

    Ok(())
}

/// Run decay sweep on startup if >24h since last sweep.
/// No background timer — the sweep runs each time the app launches (menubar app
/// starts on login, so this covers wake-from-sleep naturally).
fn setup_decay_sweep() {
    if decay::should_run_sweep().unwrap_or(false) {
        std::thread::spawn(|| {
            if let Ok(conn) = db::queries::open_connection() {
                if let Err(e) = decay::run_decay_sweep(&conn) {
                    log::error!("Startup decay sweep failed: {}", e);
                } else {
                    log::info!("Startup decay sweep completed");
                }
            }
        });
    } else {
        log::info!("Decay sweep not needed at startup");
    }
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
                // Open menubar panel (the primary UI), not the hidden main window
                if let Some(window) = app.get_webview_window("menubar") {
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
