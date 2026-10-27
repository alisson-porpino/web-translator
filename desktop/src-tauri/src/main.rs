#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod download_tracker;
mod engines;
mod http_server;
mod jobs;
mod resource_limits;
mod resource_usage;
mod settings;
mod state;
mod usage;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{Manager, WindowEvent};
use tauri_plugin_opener::OpenerExt;

use settings::Settings;
use state::{AppState, ServerHandle};
use usage::UsageTracker;

#[tauri::command]
fn get_settings(app: tauri::AppHandle) -> Settings {
    settings::load(&app)
}

#[tauri::command]
fn save_settings(app: tauri::AppHandle, settings: Settings) -> Result<(), String> {
    settings::save(&app, &settings)?;
    // Takes effect immediately, no restart needed.
    resource_limits::set_limits(settings.max_cpu_threads, settings.max_memory_mb);
    Ok(())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ServerStatus {
    running: bool,
    port: Option<u16>,
}

#[tauri::command]
fn get_status(app: tauri::AppHandle) -> serde_json::Value {
    let settings = settings::load(&app);
    serde_json::json!({ "port": settings.port, "running": true })
}

#[tauri::command]
fn get_server_status(state: tauri::State<AppState>) -> ServerStatus {
    let server = state.server.lock().unwrap();
    ServerStatus { running: server.is_some(), port: server.as_ref().map(|s| s.port) }
}

/// Starts the local HTTP server on the user's configured port (see
/// settings.rs's `port` field), binding it right here so a conflict (some
/// other app already using that port) is reported back as a real error
/// instead of only ever reaching an stderr log nobody's watching -- see
/// http_server.rs's `bind`/`serve` split and state.rs's `ServerHandle`
/// doc comment for why this isn't just done unconditionally at launch.
/// Calling this while already running is a no-op that reports the
/// existing port, not an error -- lets the UI's Start button be safely
/// clickable without first checking current status itself.
#[tauri::command]
async fn start_server(state: tauri::State<'_, AppState>, app: tauri::AppHandle) -> Result<u16, String> {
    {
        let server = state.server.lock().unwrap();
        if let Some(existing) = server.as_ref() {
            return Ok(existing.port);
        }
    }

    let port = settings::load(&app).port;
    let listener = http_server::bind(port).await.map_err(|e| match e.kind() {
        std::io::ErrorKind::AddrInUse => {
            format!("Port {port} is already in use by another application on this computer. Pick a different port in Settings and try again.")
        }
        _ => format!("Could not start the server on port {port}: {e}"),
    })?;

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    state.server.lock().unwrap().replace(ServerHandle { port, shutdown: shutdown_tx });

    let server_state = state.inner().clone();
    tauri::async_runtime::spawn(async move {
        http_server::serve(server_state, listener, shutdown_rx).await;
    });

    Ok(port)
}

/// Extracts the bundled pre-built extension package and opens the
/// resulting ready-to-load folder directly in Finder/Explorer -- no
/// manual unzip step, no build tooling, nothing for the user to run
/// themselves. CI (.github/workflows/release.yml) builds and zips both
/// browser targets ahead of time and bundles them as app resources (see
/// tauri.conf.json's `bundle.resources`). Not present in an ad hoc local
/// `cargo build`/`cargo tauri dev` run (nothing populates
/// desktop/src-tauri/resources/ outside CI) -- that's expected during
/// development, only official release downloads have it.
///
/// Always re-extracts into the same fixed folder (removing whatever was
/// there before) rather than leaving old files around from a previous
/// version of the app.
#[tauri::command]
fn install_extension(app: tauri::AppHandle, target: String) -> Result<String, String> {
    let zip_filename = match target.as_str() {
        "chrome" => "extension-chrome.zip",
        "firefox" => "extension-firefox.zip",
        other => return Err(format!("unknown extension target '{other}'")),
    };
    let zip_path = app.path().resource_dir().map_err(|e| e.to_string())?.join(zip_filename);
    if !zip_path.exists() {
        return Err(format!(
            "{zip_filename} isn't bundled with this build. This is expected in a local dev build -- \
             it's included in the official release downloads."
        ));
    }

    let extract_dir =
        app.path().app_data_dir().map_err(|e| e.to_string())?.join(format!("extension-{target}"));
    if extract_dir.exists() {
        std::fs::remove_dir_all(&extract_dir).map_err(|e| format!("could not clear {}: {e}", extract_dir.display()))?;
    }
    std::fs::create_dir_all(&extract_dir).map_err(|e| e.to_string())?;

    let file = std::fs::File::open(&zip_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("could not read {zip_filename}: {e}"))?;
    archive.extract(&extract_dir).map_err(|e| format!("could not extract {zip_filename}: {e}"))?;

    app.opener().open_path(extract_dir.display().to_string(), None::<&str>).map_err(|e| e.to_string())?;
    Ok(extract_dir.display().to_string())
}

#[tauri::command]
fn stop_server(state: tauri::State<AppState>) -> Result<(), String> {
    if let Some(handle) = state.server.lock().unwrap().take() {
        // The receiver may already be gone if the serve task ended on its
        // own (e.g. a lower-level listener error) -- not an error for the
        // caller, the server is stopped either way.
        let _ = handle.shutdown.send(());
    }
    Ok(())
}

#[tauri::command]
fn get_google_usage(state: tauri::State<AppState>, app: tauri::AppHandle) -> serde_json::Value {
    let settings = settings::load(&app);
    let (count, cap) = state.google_usage.snapshot(settings.google_daily_request_cap);
    serde_json::json!({ "count": count, "cap": cap })
}

#[tauri::command]
async fn get_resource_usage(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<resource_usage::ResourceUsage, String> {
    let settings = settings::load(&app);
    let models_loaded = engines::koharu_engine::is_loaded().await;
    let active_jobs = state.active_jobs.load(std::sync::atomic::Ordering::Relaxed);
    Ok(resource_usage::snapshot(&settings, models_loaded, active_jobs))
}

#[tauri::command]
async fn unload_models() -> Result<bool, String> {
    Ok(engines::koharu_engine::unload().await)
}

#[tauri::command]
fn get_pipeline_models() -> Result<engines::koharu_models::PipelineModels, String> {
    engines::koharu_models::get()
}

#[tauri::command]
fn set_pipeline_models(ocr: String, inpainting: String) -> Result<(), String> {
    engines::koharu_models::set(&ocr, &inpainting)
}

#[tauri::command]
async fn get_translation_models() -> Result<Vec<koharu_translator::Model>, String> {
    engines::koharu_models::list_translation_models().await
}

#[tauri::command]
fn get_translation_selection() -> Result<engines::koharu_models::TranslationSelection, String> {
    engines::koharu_models::get_translation_selection()
}

#[tauri::command]
fn set_translation_model(model: String, quantization: Option<String>) -> Result<(), String> {
    engines::koharu_models::set_translation_model(&model, quantization.as_deref())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            get_status,
            get_google_usage,
            get_resource_usage,
            unload_models,
            get_pipeline_models,
            set_pipeline_models,
            get_translation_models,
            get_translation_selection,
            set_translation_model,
            get_server_status,
            start_server,
            stop_server,
            install_extension
        ])
        .setup(|app| {
            let app_handle = app.handle().clone();
            let cache_dir = settings::cache_dir(&app_handle);
            let initial_settings = settings::load(&app_handle);
            resource_limits::set_limits(initial_settings.max_cpu_threads, initial_settings.max_memory_mb);
            download_tracker::start_download_listener();

            let state = AppState {
                app_handle: app_handle.clone(),
                jobs: Arc::new(Mutex::new(HashMap::new())),
                cache_dir,
                google_usage: Arc::new(UsageTracker::load(&app_handle)),
                active_jobs: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
                idle_timer_cancel: Arc::new(Mutex::new(None)),
                local_engine_slots: Arc::new(tokio::sync::Semaphore::new(1)),
                cancelled_jobs: Arc::new(Mutex::new(std::collections::HashSet::new())),
                running_stops: Arc::new(Mutex::new(HashMap::new())),
                server: Arc::new(Mutex::new(None)),
            };
            app.manage(state.clone());

            let open_item = MenuItem::with_id(app, "open", "Open settings", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open_item, &quit_item])?;

            TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .show_menu_on_left_click(true)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;

            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
