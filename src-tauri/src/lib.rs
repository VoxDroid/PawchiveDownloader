//! Pawchive Downloader: a desktop app for downloading from Pawchive.

pub mod app;
pub mod bridge;
pub mod commands;
pub mod core;
pub mod hosts;
pub mod jobs;
pub mod library;
pub mod logs;
pub mod naming;
pub mod net;
pub mod pawchive;
pub mod power;
pub mod settings;
pub mod store;
pub mod updates;
pub mod watch;

use std::sync::Arc;

use tauri::Manager;

use crate::core::{Core, CoreOptions};

/// Portable mode: with a `data` folder (or a file named `portable`) next to
/// the program, everything is kept there instead of in the user's profile,
/// so the whole app can live on a USB stick.
pub fn portable_dir() -> Option<std::path::PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let data = dir.join("data");
    (data.is_dir() || dir.join("portable").is_file()).then_some(data)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let minimized_arg = std::env::args().any(|a| a == "--minimized");
    let portable = portable_dir();
    if let Some(dir) = &portable {
        let _ = std::fs::create_dir_all(dir);
        // The web view keeps its own data (window size, small preferences).
        #[cfg(windows)]
        std::env::set_var("WEBVIEW2_USER_DATA_FOLDER", dir.join("webview"));
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            app::show_main(app);
            let links = app::links_in_args(&argv[1.min(argv.len())..]);
            if !links.is_empty() {
                app::emit(app, "open-links", &links.join("\n"));
            }
        }))
        .plugin(
            tauri_plugin_window_state::Builder::new()
                .with_state_flags(
                    tauri_plugin_window_state::StateFlags::all()
                        & !tauri_plugin_window_state::StateFlags::VISIBLE,
                )
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .setup(move |app| {
            let handle = app.handle().clone();
            let (data_dir, log_dir) = match &portable {
                Some(dir) => (dir.clone(), dir.join("logs")),
                None => {
                    let data_dir = app.path().app_data_dir()?;
                    let log_dir = app
                        .path()
                        .app_log_dir()
                        .unwrap_or_else(|_| data_dir.join("logs"));
                    (data_dir, log_dir)
                }
            };
            install_crash_reporter(log_dir.clone());
            let core = Core::new(CoreOptions {
                data_dir,
                log_file: Some(core::log_file_in(&log_dir)),
                origin_override: None,
                in_memory: false,
                fast_timing: false,
            })
            .map_err(Box::<dyn std::error::Error>::from)?;
            core.log.info(format!(
                "Pawchive Downloader {} started",
                env!("CARGO_PKG_VERSION")
            ));
            let log = Arc::clone(&core.log);
            let emit_handle = handle.clone();
            log.set_sink(Arc::new(move |entry| app::emit(&emit_handle, "log", entry)));
            core.engine
                .set_hooks(Arc::new(app::AppHooks::new(handle.clone())));
            let watch_handle = handle.clone();
            core.watcher.set_on_change(Arc::new(move |count| {
                let checking = watch_handle
                    .try_state::<Arc<Core>>()
                    .map(|c| {
                        c.watcher
                            .checking
                            .load(std::sync::atomic::Ordering::Relaxed)
                    })
                    .unwrap_or(false);
                app::emit(
                    &watch_handle,
                    "watch",
                    &serde_json::json!({ "newCount": count, "checking": checking }),
                );
            }));
            let queue_handle = handle.clone();
            core.set_on_queue(Arc::new(move |status| {
                app::emit(&queue_handle, "queue", &status);
            }));
            let quit_handle = handle.clone();
            core.set_on_quit(Arc::new(move || app::quit(&quit_handle)));
            let notify_handle = handle.clone();
            core.watcher.set_notify(Arc::new(move |count, names| {
                if let Some(core) = notify_handle.try_state::<Arc<Core>>() {
                    let (title, body) = app::new_posts_text(&core, count, &names);
                    app::notify(&notify_handle, &title, &body);
                }
            }));
            // Tasks can be started from any thread (menus, commands), so the
            // engine gets the runtime up front.
            let tauri::async_runtime::RuntimeHandle::Tokio(runtime) =
                tauri::async_runtime::handle();
            core.engine.attach_runtime(runtime);
            app.manage(Arc::clone(&core));
            {
                let core = Arc::clone(&core);
                tauri::async_runtime::spawn(async move { core.start() });
            }
            app::build_tray(&handle, &core)?;
            app::spawn_live_updates(handle.clone(), Arc::clone(&core));
            app::spawn_clipboard_watch(handle.clone(), Arc::clone(&core));
            let start_hidden = minimized_arg || core.settings().start_minimized;
            if !start_hidden {
                // The interface shows the window after its first paint; this
                // is the fallback should that never happen.
                let h = handle.clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_secs(4)).await;
                    if let Some(w) = h.get_webview_window("main") {
                        if !w.is_visible().unwrap_or(true) {
                            app::show_main(&h);
                        }
                    }
                });
            }
            Ok(())
        })
        .on_window_event(app::on_window_event)
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::get_settings,
            commands::update_settings,
            commands::reset_settings,
            commands::export_settings,
            commands::import_settings,
            commands::preview_templates,
            commands::check_proxy_url,
            commands::parse_links,
            commands::get_post,
            commands::get_creator,
            commands::get_linked_accounts,
            commands::search_creators,
            commands::queue_status,
            commands::library_stats,
            commands::check_library,
            commands::repair_library,
            commands::set_watch_options,
            commands::export_backup,
            commands::bridge_status,
            commands::bridge_pair,
            commands::bridge_revoke,
            commands::import_backup,
            commands::set_when_done,
            commands::cancel_when_done,
            commands::get_creator_posts,
            commands::get_recent_posts,
            commands::create_job,
            commands::queue_links,
            commands::list_jobs,
            commands::job_items,
            commands::job_action,
            commands::jobs_action_all,
            commands::clear_finished,
            commands::retry_item,
            commands::list_history,
            commands::remove_history,
            commands::clear_history,
            commands::export_history_csv,
            commands::redownload,
            commands::library_counts,
            commands::forget_known,
            commands::list_watch,
            commands::add_watch,
            commands::remove_watch,
            commands::set_watch_auto,
            commands::mark_watch_seen,
            commands::check_watch,
            commands::queue_watch_new,
            commands::list_favorites,
            commands::add_favorite,
            commands::update_favorite,
            commands::remove_favorites,
            commands::queue_favorites,
            commands::export_favorites,
            commands::import_favorites,
            commands::get_logs,
            commands::clear_logs,
            commands::export_logs,
            commands::open_path,
            commands::reveal_path,
            commands::path_exists,
            commands::ensure_dir,
            commands::read_text_file,
            commands::write_text_file,
            commands::check_updates,
            commands::ack_crash,
            ui_ready,
            quit_app,
        ])
        .build(tauri::generate_context!())
        .expect("error while building the app")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit = event {
                if let Some(core) = app.try_state::<Arc<Core>>() {
                    core.shutdown();
                }
            }
        });
}

/// Writes a report for any panic to `crash-<time>.txt` in the log folder.
/// Background tasks survive panics (they are caught and reported as failed
/// downloads); the report is what lets a bug be found and fixed.
fn install_crash_reporter(dir: std::path::PathBuf) {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let now = chrono::Local::now();
        let text = format!(
            "Pawchive Downloader {} ({}, {} {})
{}

{}

{}
",
            env!("CARGO_PKG_VERSION"),
            updates::channel(),
            std::env::consts::OS,
            std::env::consts::ARCH,
            now.to_rfc3339(),
            info,
            std::backtrace::Backtrace::force_capture()
        );
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(
            dir.join(format!("crash-{}.txt", now.format("%Y%m%d-%H%M%S"))),
            text,
        );
        default_hook(info);
    }));
}

/// Called by the interface after its first paint: shows the window (unless
/// the app was started hidden in the tray).
#[tauri::command]
fn ui_ready(app: tauri::AppHandle, window: tauri::WebviewWindow) {
    let minimized = std::env::args().any(|a| a == "--minimized");
    let hidden = app
        .try_state::<Arc<Core>>()
        .map(|c| c.settings().start_minimized)
        .unwrap_or(false);
    if !(minimized || hidden) {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[tauri::command]
fn quit_app(app: tauri::AppHandle) {
    app::quit(&app);
}
