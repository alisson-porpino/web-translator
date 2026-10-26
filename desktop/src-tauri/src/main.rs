#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

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

use settings::Settings;
use state::AppState;
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

#[tauri::command]
fn get_status(app: tauri::AppHandle) -> serde_json::Value {
    let settings = settings::load(&app);
    serde_json::json!({ "port": settings.port, "running": true })
}

#[tauri::command]
fn get_google_usage(state: tauri::State<AppState>, app: tauri::AppHandle) -> serde_json::Value {
    let settings = settings::load(&app);
    let (count, cap) = state.google_usage.snapshot(settings.google_daily_request_cap);
    serde_json::json!({ "count": count, "cap": cap })
}

#[tauri::command]
fn get_resource_usage(app: tauri::AppHandle) -> resource_usage::ResourceUsage {
    resource_usage::snapshot(&settings::load(&app))
}

#[tauri::command]
fn get_pipeline_models() -> Result<engines::koharu_models::PipelineModels, String> {
    engines::koharu_models::get()
}

#[tauri::command]
fn set_pipeline_models(ocr: String, inpainting: String) -> Result<(), String> {
    engines::koharu_models::set(&ocr, &inpainting)
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            get_status,
            get_google_usage,
            get_resource_usage,
            get_pipeline_models,
            set_pipeline_models
        ])
        .setup(|app| {
            let app_handle = app.handle().clone();
            let cache_dir = settings::cache_dir(&app_handle);
            let initial_settings = settings::load(&app_handle);
            resource_limits::set_limits(initial_settings.max_cpu_threads, initial_settings.max_memory_mb);

            let state = AppState {
                app_handle: app_handle.clone(),
                jobs: Arc::new(Mutex::new(HashMap::new())),
                cache_dir,
                google_usage: Arc::new(UsageTracker::load(&app_handle)),
                local_engine_slots: Arc::new(tokio::sync::Semaphore::new(1)),
                cancelled_jobs: Arc::new(Mutex::new(std::collections::HashSet::new())),
                running_stops: Arc::new(Mutex::new(HashMap::new())),
            };
            app.manage(state.clone());

            let server_state = state.clone();
            let port = initial_settings.port;
            tauri::async_runtime::spawn(async move {
                http_server::run(server_state, port).await;
            });

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
