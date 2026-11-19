//! The desktop shell: window, tray icon, notifications, live updates to the
//! interface, start at login and the single-instance handoff.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use tauri::menu::{Menu, MenuBuilder, MenuItemBuilder, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::window::{ProgressBarState, ProgressBarStatus};
use tauri::{AppHandle, Emitter, Manager, Runtime, WindowEvent};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_notification::NotificationExt;

use crate::core::Core;
use crate::jobs::engine::Hooks;
use crate::jobs::model::{Job, JobStatus};
use crate::store::records::HistoryEntry;

pub const TRAY_ID: &str = "main";
static TRAY_HINT_SHOWN: AtomicBool = AtomicBool::new(false);
static QUITTING: AtomicBool = AtomicBool::new(false);

pub fn emit<S: Serialize + Clone>(app: &AppHandle, event: &str, payload: &S) {
    let _ = app.emit(event, payload.clone());
}

/// The few strings the native side shows (tray menu, notifications).
struct Words {
    show: &'static str,
    pause: &'static str,
    resume: &'static str,
    check: &'static str,
    folder: &'static str,
    quit: &'static str,
    finished: &'static str,
    failed: &'static str,
    new_posts: &'static str,
    tray_title: &'static str,
    tray_body: &'static str,
    all_done: &'static str,
    batch_body: &'static str,
    batch_failed: &'static str,
}

fn words(lang: &str) -> Words {
    match lang {
        "ja" => Words {
            show: "ウィンドウを表示",
            pause: "すべて一時停止",
            resume: "すべて再開",
            check: "ウォッチリストを確認",
            folder: "ダウンロードフォルダーを開く",
            quit: "終了",
            finished: "ダウンロード完了",
            failed: "エラーありで完了",
            new_posts: "件の新しい投稿",
            tray_title: "バックグラウンドで実行中",
            tray_body: "トレイアイコンから再度開けます。",
            all_done: "すべてのダウンロードが完了",
            batch_body: "{jobs} 件のダウンロード、{files} 個のファイルを保存",
            batch_failed: "{failed} 個のファイルをダウンロードできませんでした",
        },
        "ko" => Words {
            show: "창 표시",
            pause: "모두 일시 정지",
            resume: "모두 재개",
            check: "관심 목록 확인",
            folder: "다운로드 폴더 열기",
            quit: "종료",
            finished: "다운로드 완료",
            failed: "오류와 함께 완료",
            new_posts: "개의 새 게시물",
            tray_title: "백그라운드에서 실행 중",
            tray_body: "트레이 아이콘에서 다시 열 수 있습니다.",
            all_done: "모든 다운로드 완료",
            batch_body: "다운로드 {jobs}개, 파일 {files}개 저장",
            batch_failed: "파일 {failed}개를 다운로드하지 못했습니다",
        },
        "zh_CN" => Words {
            show: "显示窗口",
            pause: "全部暂停",
            resume: "全部继续",
            check: "检查关注列表",
            folder: "打开下载文件夹",
            quit: "退出",
            finished: "下载完成",
            failed: "完成但有错误",
            new_posts: "个新帖子",
            tray_title: "正在后台运行",
            tray_body: "可从托盘图标重新打开。",
            all_done: "全部下载完成",
            batch_body: "{jobs} 个下载，已保存 {files} 个文件",
            batch_failed: "{failed} 个文件无法下载",
        },
        "es" => Words {
            show: "Mostrar ventana",
            pause: "Pausar todo",
            resume: "Reanudar todo",
            check: "Revisar seguimiento",
            folder: "Abrir carpeta de descargas",
            quit: "Salir",
            finished: "Descarga terminada",
            failed: "Terminada con errores",
            new_posts: "publicaciones nuevas",
            tray_title: "Sigue en segundo plano",
            tray_body: "Vuelve a abrirlo desde el icono de la bandeja.",
            all_done: "Todas las descargas terminaron",
            batch_body: "{jobs} descargas, {files} archivos guardados",
            batch_failed: "{failed} archivos no se pudieron descargar",
        },
        "pt_BR" => Words {
            show: "Mostrar janela",
            pause: "Pausar tudo",
            resume: "Retomar tudo",
            check: "Verificar lista",
            folder: "Abrir pasta de downloads",
            quit: "Sair",
            finished: "Download concluído",
            failed: "Concluído com erros",
            new_posts: "novas publicações",
            tray_title: "Rodando em segundo plano",
            tray_body: "Abra de novo pelo ícone da bandeja.",
            all_done: "Todos os downloads concluídos",
            batch_body: "{jobs} downloads, {files} arquivos salvos",
            batch_failed: "{failed} arquivos não puderam ser baixados",
        },
        "ru" => Words {
            show: "Показать окно",
            pause: "Приостановить все",
            resume: "Возобновить все",
            check: "Проверить отслеживание",
            folder: "Открыть папку загрузок",
            quit: "Выход",
            finished: "Загрузка завершена",
            failed: "Завершено с ошибками",
            new_posts: "новых публикаций",
            tray_title: "Работает в фоне",
            tray_body: "Откройте снова через значок в трее.",
            all_done: "Все загрузки завершены",
            batch_body: "Загрузок: {jobs}, сохранено файлов: {files}",
            batch_failed: "Не удалось скачать файлов: {failed}",
        },
        "de" => Words {
            show: "Fenster anzeigen",
            pause: "Alle pausieren",
            resume: "Alle fortsetzen",
            check: "Beobachtungsliste prüfen",
            folder: "Download-Ordner öffnen",
            quit: "Beenden",
            finished: "Download fertig",
            failed: "Mit Fehlern beendet",
            new_posts: "neue Beiträge",
            tray_title: "Läuft im Hintergrund",
            tray_body: "Über das Symbol im Infobereich wieder öffnen.",
            all_done: "Alle Downloads fertig",
            batch_body: "{jobs} Downloads, {files} Dateien gespeichert",
            batch_failed: "{failed} Dateien konnten nicht geladen werden",
        },
        "fr" => Words {
            show: "Afficher la fenêtre",
            pause: "Tout suspendre",
            resume: "Tout reprendre",
            check: "Vérifier la liste de suivi",
            folder: "Ouvrir le dossier de téléchargement",
            quit: "Quitter",
            finished: "Téléchargement terminé",
            failed: "Terminé avec des erreurs",
            new_posts: "nouvelles publications",
            tray_title: "Toujours en arrière-plan",
            tray_body: "Rouvrez-le depuis l'icône de la barre d'état.",
            all_done: "Tous les téléchargements sont terminés",
            batch_body: "{jobs} téléchargements, {files} fichiers enregistrés",
            batch_failed: "{failed} fichiers n'ont pas pu être téléchargés",
        },
        _ => Words {
            show: "Show window",
            pause: "Pause all",
            resume: "Resume all",
            check: "Check watchlist now",
            folder: "Open download folder",
            quit: "Quit",
            finished: "Download finished",
            failed: "Finished with errors",
            new_posts: "new posts",
            tray_title: "Still running in the background",
            tray_body: "Open it again from the tray icon.",
            all_done: "All downloads finished",
            batch_body: "{jobs} downloads, {files} files saved",
            batch_failed: "{failed} files could not be downloaded",
        },
    }
}

/// The UI language from settings, resolving "auto" from the system locale.
pub fn language(core: &Core) -> String {
    let lang = core.settings().language;
    if lang != "auto" {
        return lang;
    }
    let sys = sys_locale().to_lowercase().replace('-', "_");
    if sys.starts_with("zh") {
        return "zh_CN".into();
    }
    if sys.starts_with("pt") {
        return "pt_BR".into();
    }
    let base = sys.split('_').next().unwrap_or("en").to_string();
    if crate::settings::LOCALES.contains(&base.as_str()) {
        base
    } else {
        "en".into()
    }
}

fn sys_locale() -> String {
    for var in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(v) = std::env::var(var) {
            if !v.is_empty() {
                return v;
            }
        }
    }
    #[cfg(windows)]
    {
        if let Ok(out) = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", "(Get-Culture).Name"])
            .creation_flags(0x08000000)
            .output()
        {
            return String::from_utf8_lossy(&out.stdout).trim().to_string();
        }
    }
    "en".into()
}

#[cfg(windows)]
use std::os::windows::process::CommandExt;

pub fn notify(app: &AppHandle, title: &str, body: &str) {
    let _ = app.notification().builder().title(title).body(body).show();
}

pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

pub fn set_autostart(app: &AppHandle, on: bool) {
    let launcher = app.autolaunch();
    let _ = if on {
        launcher.enable()
    } else {
        launcher.disable()
    };
}

pub fn quit(app: &AppHandle) {
    QUITTING.store(true, Ordering::Relaxed);
    if let Some(core) = app.try_state::<Arc<Core>>() {
        core.shutdown();
    }
    app.exit(0);
}

fn tray_menu<R: Runtime>(app: &AppHandle<R>, lang: &str) -> tauri::Result<Menu<R>> {
    let w = words(lang);
    MenuBuilder::new(app)
        .item(&MenuItemBuilder::with_id("show", w.show).build(app)?)
        .item(&PredefinedMenuItem::separator(app)?)
        .item(&MenuItemBuilder::with_id("pause", w.pause).build(app)?)
        .item(&MenuItemBuilder::with_id("resume", w.resume).build(app)?)
        .item(&MenuItemBuilder::with_id("check", w.check).build(app)?)
        .item(&MenuItemBuilder::with_id("folder", w.folder).build(app)?)
        .item(&PredefinedMenuItem::separator(app)?)
        .item(&MenuItemBuilder::with_id("quit", w.quit).build(app)?)
        .build()
}

pub fn refresh_tray_menu(app: &AppHandle) {
    let Some(core) = app.try_state::<Arc<Core>>() else {
        return;
    };
    if let (Some(tray), Ok(menu)) = (app.tray_by_id(TRAY_ID), tray_menu(app, &language(&core))) {
        let _ = tray.set_menu(Some(menu));
    }
}

pub fn build_tray(app: &AppHandle, core: &Arc<Core>) -> tauri::Result<()> {
    let menu = tray_menu(app, &language(core))?;
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("Pawchive Downloader")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            let Some(core) = app.try_state::<Arc<Core>>() else {
                return;
            };
            match event.id().as_ref() {
                "show" => show_main(app),
                "pause" => core.engine.act_all(crate::jobs::engine::JobAction::Pause),
                "resume" => core.engine.act_all(crate::jobs::engine::JobAction::Resume),
                "check" => {
                    let core = Arc::clone(&core);
                    tauri::async_runtime::spawn(async move {
                        let cancel = core.cancel.child_token();
                        core.watcher.check(None, &cancel).await;
                    });
                }
                "folder" => {
                    let dir = core.settings().download_dir;
                    let _ = std::fs::create_dir_all(&dir);
                    use tauri_plugin_opener::OpenerExt;
                    let _ = app.opener().open_path(dir, None::<&str>);
                }
                "quit" => quit(app),
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

/// Keeps the window open in the tray when the user closes it, if asked to.
pub fn on_window_event(window: &tauri::Window, event: &WindowEvent) {
    if let WindowEvent::CloseRequested { api, .. } = event {
        if QUITTING.load(Ordering::Relaxed) {
            return;
        }
        let app = window.app_handle();
        let Some(core) = app.try_state::<Arc<Core>>() else {
            return;
        };
        if core.settings().close_to_tray {
            api.prevent_close();
            let _ = window.hide();
            if !TRAY_HINT_SHOWN.swap(true, Ordering::Relaxed) && core.engine.busy_count() > 0 {
                let w = words(&language(&core));
                notify(app, w.tray_title, w.tray_body);
            }
        } else {
            quit(app);
        }
    }
}

pub struct AppHooks {
    pub app: AppHandle,
    /// Jobs finished since the queue was last idle, for one summary.
    pub batch: parking_lot::Mutex<Batch>,
}

#[derive(Default)]
pub struct Batch {
    jobs: u32,
    files: u32,
    failed: u32,
}

impl AppHooks {
    pub fn new(app: AppHandle) -> Self {
        Self {
            app,
            batch: parking_lot::Mutex::new(Batch::default()),
        }
    }
}

impl Hooks for AppHooks {
    /// One notification per batch: while other downloads are still running,
    /// finished jobs are counted, and the summary comes when the queue is idle.
    /// Called with the engine locked, so the notification is shown from a
    /// separate task.
    fn job_finished(&self, job: &Job, entry: &HistoryEntry, still_running: usize) {
        let Some(core) = self.app.try_state::<Arc<Core>>() else {
            return;
        };
        let level = core.settings().notifications;
        if level == "none" {
            return;
        }
        let (jobs, files, failed) = {
            let mut batch = self.batch.lock();
            if job.status != JobStatus::Cancelled {
                batch.jobs += 1;
                batch.files += entry.files.done;
                batch.failed += entry.files.failed;
            }
            if still_running > 0 {
                return;
            }
            let totals = (batch.jobs, batch.files, batch.failed);
            *batch = Batch::default();
            totals
        };
        if jobs == 0 || (level == "errors" && failed == 0) {
            return;
        }
        let w = words(&language(&core));
        // Private notifications never show titles or creator names.
        let private = core.settings().private_notifications;
        let (title, body) = if jobs == 1 && !private {
            let title = if failed > 0 { w.failed } else { w.finished };
            let body = format!(
                "{} ({}/{})",
                job.title,
                entry.files.done,
                entry.files.done + entry.files.failed + entry.files.skipped
            );
            (title.to_string(), body)
        } else {
            let mut body = w
                .batch_body
                .replace("{jobs}", &jobs.to_string())
                .replace("{files}", &files.to_string());
            if failed > 0 {
                body.push('\n');
                body.push_str(&w.batch_failed.replace("{failed}", &failed.to_string()));
            }
            (w.all_done.to_string(), body)
        };
        let app = self.app.clone();
        tauri::async_runtime::spawn(async move { notify(&app, &title, &body) });
    }
}

/// Pushes the job list to the interface while anything changes, and mirrors
/// overall progress on the tray tooltip and the taskbar button.
pub fn spawn_live_updates(app: AppHandle, core: Arc<Core>) {
    tauri::async_runtime::spawn(async move {
        let mut last = 0;
        let mut was_busy = false;
        let mut tick = tokio::time::interval(Duration::from_millis(250));
        loop {
            tick.tick().await;
            if core.cancel.is_cancelled() {
                break;
            }
            let version = core.engine.version();
            let busy = core.engine.busy_count() > 0;
            if version == last && !busy && !was_busy {
                continue;
            }
            last = version;
            was_busy = busy;
            let jobs = core.engine.summaries();
            let (mut done, mut total, mut speed, mut running) = (0.0, 0.0, 0.0, 0);
            for j in jobs.iter().filter(|j| {
                matches!(
                    j.status,
                    JobStatus::Running | JobStatus::Queued | JobStatus::Resolving
                )
            }) {
                running += 1;
                done +=
                    f64::from(j.stats.done + j.stats.failed + j.stats.skipped) + j.stats.partial;
                total += f64::from(j.stats.total);
                speed += j.speed;
            }
            emit(&app, "jobs", &jobs);
            if let Some(tray) = app.tray_by_id(TRAY_ID) {
                let tip = if running > 0 {
                    format!(
                        "Pawchive Downloader\n{running} running, {:.0}%, {}/s",
                        if total > 0.0 {
                            done / total * 100.0
                        } else {
                            0.0
                        },
                        human_bytes(speed)
                    )
                } else {
                    "Pawchive Downloader".to_string()
                };
                let _ = tray.set_tooltip(Some(tip));
            }
            if let Some(window) = app.get_webview_window("main") {
                let state = if running > 0 && total > 0.0 {
                    ProgressBarState {
                        status: Some(ProgressBarStatus::Normal),
                        progress: Some(((done / total) * 100.0).round() as u64),
                    }
                } else {
                    ProgressBarState {
                        status: Some(ProgressBarStatus::None),
                        progress: None,
                    }
                };
                let _ = window.set_progress_bar(state);
            }
        }
    });
}

pub fn human_bytes(n: f64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = n.max(0.0);
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{v:.0} {}", UNITS[i])
    } else {
        format!("{v:.1} {}", UNITS[i])
    }
}

/// Pawchive links passed on the command line (or by a second launch).
pub fn links_in_args(args: &[String]) -> Vec<String> {
    args.iter()
        .filter(|a| !a.starts_with("--"))
        .filter(|a| {
            url::Url::parse(a)
                .ok()
                .and_then(|u| u.host_str().map(crate::pawchive::urls::is_site_host))
                .unwrap_or(false)
        })
        .cloned()
        .collect()
}

pub fn new_posts_text(core: &Core, count: usize, names: &str) -> (String, String) {
    let w = words(&language(core));
    let names = if core.settings().private_notifications {
        String::new()
    } else {
        names.to_string()
    };
    (format!("{count} {}", w.new_posts), names)
}

/// Watches the clipboard while the setting is on and tells the window about
/// newly copied links to a supported site. Only the text is read, every 1.5 s,
/// and nothing leaves the computer.
pub fn spawn_clipboard_watch(app: AppHandle, core: Arc<Core>) {
    use tauri_plugin_clipboard_manager::ClipboardExt;
    tauri::async_runtime::spawn(async move {
        let mut last = String::new();
        let mut was_on = false;
        let mut tick = tokio::time::interval(Duration::from_millis(1500));
        loop {
            tick.tick().await;
            if core.cancel.is_cancelled() {
                break;
            }
            let on = core.settings().watch_clipboard;
            let text = if on {
                app.clipboard().read_text().unwrap_or_default()
            } else {
                String::new()
            };
            // What was already copied when watching starts is not new.
            if on && !was_on {
                last = text.clone();
            }
            was_on = on;
            if !on || text == last {
                continue;
            }
            last = text.clone();
            let links: Vec<String> = crate::pawchive::urls::extract_links(&text)
                .into_iter()
                .map(|(url, _)| url)
                .take(200)
                .collect();
            if !links.is_empty() {
                emit(
                    &app,
                    "clipboard-links",
                    &links.join(
                        "
",
                    ),
                );
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_and_filters() {
        assert_eq!(human_bytes(512.0), "512 B");
        assert_eq!(human_bytes(1536.0), "1.5 KB");
        assert_eq!(
            links_in_args(&[
                "--minimized".into(),
                "https://pawchive.pw/a/user/1".into(),
                "x".into()
            ])
            .len(),
            1
        );
        for lang in crate::settings::LOCALES {
            assert!(!words(lang).quit.is_empty());
        }
    }
}
