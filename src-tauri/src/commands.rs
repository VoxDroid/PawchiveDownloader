//! The commands the interface calls. Every one returns plain data or a short
//! error message the UI can show.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;
use tokio_util::sync::CancellationToken;

use crate::core::Core;
use crate::jobs::engine::{now_ms, JobAction};
use crate::jobs::model::{CreatorInfo, ItemView, JobOverrides, JobRequest, JobSummary, PostRef};
use crate::logs::LogEntry;
use crate::naming::template::{
    build_archive_path, build_file_path, validate_template, NameContext,
};
use crate::net::http::{user_agent, validate_proxy_url};
use crate::pawchive::client::ApiError;
use crate::pawchive::types::{Creator, Post};
use crate::pawchive::urls::{extract_links, parse_page_url, PageRef};
use crate::settings::{self, Settings};
use crate::store::records::{history_to_csv, Favorite, FavoriteKind, HistoryEntry, WatchEntry};
use crate::store::Counts;
use crate::updates::{self, UpdateInfo};
use crate::watch::CheckReport;

pub type Core_ = Arc<Core>;
type R<T> = Result<T, String>;

fn api(e: ApiError) -> String {
    match e {
        ApiError::Status { status: 404, .. } => "not-found".into(),
        other => other.to_string(),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    version: String,
    platform: String,
    arch: String,
    data_dir: String,
    log_file: Option<String>,
    user_agent: String,
    args: Vec<String>,
    /// "official" for Ko-fi builds, "source" for builds made from the code.
    channel: &'static str,
}

#[tauri::command]
pub fn app_info(core: State<'_, Core_>) -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION").into(),
        platform: std::env::consts::OS.into(),
        arch: std::env::consts::ARCH.into(),
        data_dir: core.data_dir.to_string_lossy().into_owned(),
        log_file: core
            .log
            .file_path()
            .map(|p| p.to_string_lossy().into_owned()),
        user_agent: user_agent(),
        args: std::env::args()
            .skip(1)
            .filter(|a| !a.starts_with("--"))
            .collect(),
        channel: crate::updates::channel(),
    }
}

// ------------------------------------------------------------------ settings

#[tauri::command]
pub fn get_settings(core: State<'_, Core_>) -> Settings {
    core.settings()
}

#[tauri::command]
pub fn update_settings(app: AppHandle, core: State<'_, Core_>, patch: Value) -> R<Settings> {
    let previous = core.settings();
    let next = core.apply_settings(settings::merge(&previous, &patch))?;
    if previous.launch_at_login != next.launch_at_login {
        crate::app::set_autostart(&app, next.launch_at_login);
    }
    if previous.language != next.language {
        crate::app::refresh_tray_menu(&app);
    }
    crate::app::emit(&app, "settings", &next);
    Ok(next)
}

#[tauri::command]
pub fn reset_settings(app: AppHandle, core: State<'_, Core_>) -> R<Settings> {
    let previous = core.settings();
    // Keep the things a reset would be annoying to lose.
    let next = Settings {
        download_dir: previous.download_dir,
        onboarded: true,
        ..Settings::default()
    };
    let next = core.apply_settings(next)?;
    crate::app::emit(&app, "settings", &next);
    Ok(next)
}

#[tauri::command]
pub fn export_settings(core: State<'_, Core_>, path: String) -> R<()> {
    let text = serde_json::to_string_pretty(&core.settings()).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn import_settings(app: AppHandle, core: State<'_, Core_>, path: String) -> R<Settings> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let value: Value = serde_json::from_str(&text).map_err(|_| "invalid-file".to_string())?;
    let next = core.apply_settings(settings::merge(&core.settings(), &value))?;
    crate::app::emit(&app, "settings", &next);
    Ok(next)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplatePreview {
    file: String,
    archive: String,
    folder_error: Option<&'static str>,
    file_error: Option<&'static str>,
    archive_error: Option<&'static str>,
}

/// Shows what the templates produce for a sample post.
#[tauri::command]
pub fn preview_templates(
    folder: String,
    file: String,
    archive: String,
    sample: Option<NameContext>,
) -> TemplatePreview {
    let ctx = sample.unwrap_or_else(|| NameContext {
        service: "patreon".into(),
        creator_id: "12345".into(),
        creator_name: "Aster Vale".into(),
        post_id: "987654".into(),
        post_title: "Spring Sketchbook".into(),
        published: Some("2026-04-12T09:30:00".into()),
        added: Some("2026-04-13T10:00:00".into()),
        index: 3,
        count: 12,
        file_name: "forest-morning.png".into(),
        ext: "png".into(),
        hash: Some("64bb0f5c2a91e4d7b3a8c6f1e2d9b7a4c5e8f3d1b6a9c2e7f4d8b1a3c6e9f2d5".into()),
        role: "attachment".into(),
        now_ms: Some(now_ms()),
    });
    TemplatePreview {
        file: build_file_path(&folder, &file, &ctx),
        archive: build_archive_path(&folder, &archive, &ctx),
        folder_error: validate_template(&folder, "folder"),
        file_error: validate_template(&file, "file"),
        archive_error: validate_template(&archive, "file"),
    }
}

#[tauri::command]
pub fn check_proxy_url(url: String) -> Option<String> {
    validate_proxy_url(&url).err()
}

// ------------------------------------------------------------------- browse

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedLink {
    url: String,
    #[serde(rename = "ref")]
    page: PageRef,
}

#[tauri::command]
pub fn parse_links(text: String) -> Vec<ParsedLink> {
    let mut links: Vec<ParsedLink> = extract_links(&text)
        .into_iter()
        .map(|(url, page)| ParsedLink { url, page })
        .collect();
    if links.is_empty() {
        let page = parse_page_url(&text);
        if page != PageRef::Other {
            links.push(ParsedLink {
                url: text.trim().into(),
                page,
            });
        }
    }
    links
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PostView {
    post: Post,
    creator: Option<Creator>,
    /// Hashes of this post's files already downloaded.
    known: Vec<String>,
    favorite: bool,
}

#[tauri::command]
pub async fn get_post(
    core: State<'_, Core_>,
    service: String,
    creator_id: String,
    post_id: String,
) -> R<PostView> {
    let cancel = CancellationToken::new();
    let post = core
        .client
        .get_post(&service, &creator_id, &post_id, &cancel)
        .await
        .map_err(api)?;
    let creator = core
        .client
        .get_creator(&service, &creator_id, &cancel)
        .await
        .ok();
    let hashes: Vec<String> = post.files.iter().filter_map(|f| f.hash.clone()).collect();
    let known = core.store.known_hashes(&hashes).into_iter().collect();
    let favorite = core
        .store
        .list_favorites()
        .iter()
        .any(|f| f.key == format!("p:{service}:{creator_id}:{post_id}"));
    Ok(PostView {
        post,
        creator,
        known,
        favorite,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatorView {
    creator: Creator,
    watched: bool,
    favorite: bool,
}

#[tauri::command]
pub async fn get_creator(
    core: State<'_, Core_>,
    service: String,
    creator_id: String,
) -> R<CreatorView> {
    let cancel = CancellationToken::new();
    let creator = core
        .client
        .get_creator(&service, &creator_id, &cancel)
        .await
        .map_err(api)?;
    let watched = core
        .store
        .list_watch()
        .iter()
        .any(|w| w.service == service && w.creator_id == creator_id);
    let favorite = core
        .store
        .list_favorites()
        .iter()
        .any(|f| f.key == format!("c:{service}:{creator_id}"));
    Ok(CreatorView {
        creator,
        watched,
        favorite,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PostPage {
    posts: Vec<Post>,
    /// Ids of posts already downloaded completely.
    downloaded: Vec<String>,
}

fn post_page(core: &Core, posts: Vec<Post>) -> PostPage {
    let keys: Vec<String> = posts
        .iter()
        .map(|p| format!("{}:{}:{}", p.service, p.creator_id, p.id))
        .collect();
    let known = core.store.known_posts(&keys);
    let downloaded = posts
        .iter()
        .filter(|p| known.contains(&format!("{}:{}:{}", p.service, p.creator_id, p.id)))
        .map(|p| p.id.clone())
        .collect();
    PostPage { posts, downloaded }
}

#[tauri::command]
pub async fn get_creator_posts(
    core: State<'_, Core_>,
    service: String,
    creator_id: String,
    offset: u32,
    query: Option<String>,
) -> R<PostPage> {
    let cancel = CancellationToken::new();
    let posts = core
        .client
        .get_creator_page(
            &service,
            &creator_id,
            offset,
            query.as_deref().filter(|q| !q.is_empty()),
            &cancel,
        )
        .await
        .map_err(api)?;
    Ok(post_page(&core, posts))
}

#[tauri::command]
pub async fn get_recent_posts(
    core: State<'_, Core_>,
    offset: u32,
    query: Option<String>,
) -> R<PostPage> {
    let cancel = CancellationToken::new();
    let posts = core
        .client
        .get_recent_posts(offset, query.as_deref().filter(|q| !q.is_empty()), &cancel)
        .await
        .map_err(api)?;
    Ok(post_page(&core, posts))
}

// --------------------------------------------------------------------- jobs

#[tauri::command]
pub fn create_job(core: State<'_, Core_>, request: JobRequest) -> String {
    core.engine.create(request)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchResult {
    url: String,
    job_id: Option<String>,
    error: Option<String>,
}

/// Queues every Pawchive link in a block of text: posts of the same creator
/// become one job, each creator link its own job.
#[tauri::command]
pub fn queue_links(
    core: State<'_, Core_>,
    text: String,
    options: Option<JobOverrides>,
) -> Vec<BatchResult> {
    let options = options.unwrap_or_default();
    let mut results = Vec::new();
    let mut posts: Vec<(String, PostRef)> = Vec::new();
    for (url, page) in extract_links(&text) {
        match page {
            PageRef::Post {
                service,
                creator_id,
                post_id,
            } => posts.push((
                url,
                PostRef {
                    service,
                    creator_id,
                    post_id,
                },
            )),
            PageRef::Creator {
                service,
                creator_id,
                query,
                ..
            } => {
                let id = core.engine.create(JobRequest::Creator {
                    service,
                    creator_id,
                    query,
                    post_ids: None,
                    since: None,
                    until: None,
                    options: options.clone(),
                });
                results.push(BatchResult {
                    url,
                    job_id: Some(id),
                    error: None,
                });
            }
            _ => results.push(BatchResult {
                url,
                job_id: None,
                error: Some("unsupported".into()),
            }),
        }
    }
    let mut groups: Vec<(String, Vec<(String, PostRef)>)> = Vec::new();
    for (url, r) in posts {
        let key = format!("{}:{}", r.service, r.creator_id);
        match groups.iter_mut().find(|(k, _)| *k == key) {
            Some((_, list)) => list.push((url, r)),
            None => groups.push((key, vec![(url, r)])),
        }
    }
    for (_, list) in groups {
        let refs: Vec<PostRef> = list.iter().map(|(_, r)| r.clone()).collect();
        let id = core.engine.create(JobRequest::Refs {
            title: None,
            creator: None,
            refs,
            options: options.clone(),
        });
        for (url, _) in list {
            results.push(BatchResult {
                url,
                job_id: Some(id.clone()),
                error: None,
            });
        }
    }
    results
}

#[tauri::command]
pub fn list_jobs(core: State<'_, Core_>) -> Vec<JobSummary> {
    core.engine.summaries()
}

#[tauri::command]
pub fn job_items(core: State<'_, Core_>, job_id: String) -> Vec<ItemView> {
    core.engine.items(&job_id)
}

#[tauri::command]
pub fn job_action(core: State<'_, Core_>, job_id: String, action: JobAction) {
    core.engine.act(&job_id, action);
}

#[tauri::command]
pub fn jobs_action_all(core: State<'_, Core_>, action: JobAction) {
    core.engine.act_all(action);
}

#[tauri::command]
pub fn clear_finished(core: State<'_, Core_>) {
    core.engine.clear_finished();
}

#[tauri::command]
pub fn retry_item(core: State<'_, Core_>, job_id: String, item_id: String) {
    core.engine.retry_item(&job_id, &item_id);
}

// ------------------------------------------------------------------ history

#[tauri::command]
pub fn list_history(core: State<'_, Core_>) -> Vec<HistoryEntry> {
    core.store.list_history()
}

#[tauri::command]
pub fn remove_history(core: State<'_, Core_>, ids: Vec<String>) -> R<()> {
    core.store.remove_history(&ids)
}

#[tauri::command]
pub fn clear_history(core: State<'_, Core_>, also_known_files: bool) -> R<()> {
    core.store.clear_history(also_known_files)
}

#[tauri::command]
pub fn export_history_csv(core: State<'_, Core_>, path: String) -> R<()> {
    std::fs::write(path, history_to_csv(&core.store.list_history())).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn redownload(core: State<'_, Core_>, id: String) -> R<String> {
    let entry = core.store.get_history(&id).ok_or("not-found")?;
    if entry.refs.is_empty() {
        return Err("nothing-to-download".into());
    }
    Ok(core.engine.create(JobRequest::Refs {
        title: Some(entry.title),
        creator: entry.creator,
        refs: entry.refs,
        options: JobOverrides {
            mode: Some(entry.mode),
            skip_downloaded: Some(false),
            ..Default::default()
        },
    }))
}

#[tauri::command]
pub fn library_counts(core: State<'_, Core_>) -> Counts {
    core.store.counts()
}

#[tauri::command]
pub fn forget_known(core: State<'_, Core_>) -> R<()> {
    core.store.forget_known()
}

// ---------------------------------------------------------------- watchlist

#[tauri::command]
pub fn list_watch(core: State<'_, Core_>) -> Vec<WatchEntry> {
    core.watcher.list()
}

#[tauri::command]
pub fn add_watch(
    core: State<'_, Core_>,
    service: String,
    creator_id: String,
    name: String,
) -> R<bool> {
    core.watcher.add(&service, &creator_id, &name)
}

#[tauri::command]
pub fn remove_watch(core: State<'_, Core_>, keys: Vec<String>) -> R<()> {
    core.watcher
        .update(|entries| entries.retain(|e| !keys.contains(&e.key)))
}

#[tauri::command]
pub fn set_watch_auto(core: State<'_, Core_>, key: String, on: bool) -> R<()> {
    core.watcher.update(|entries| {
        if let Some(e) = entries.iter_mut().find(|e| e.key == key) {
            e.auto_download = on;
        }
    })
}

#[tauri::command]
pub fn mark_watch_seen(
    core: State<'_, Core_>,
    key: Option<String>,
    ids: Option<Vec<String>>,
) -> R<()> {
    core.watcher.update(|entries| {
        for e in entries
            .iter_mut()
            .filter(|e| key.as_ref().is_none_or(|k| *k == e.key))
        {
            e.mark_seen(ids.as_deref());
        }
    })
}

#[tauri::command]
pub async fn check_watch(core: State<'_, Core_>, keys: Option<Vec<String>>) -> R<CheckReport> {
    let cancel = core.cancel.child_token();
    Ok(core.watcher.check(keys, &cancel).await)
}

/// Queues the new posts of one watched creator (or the given ones).
#[tauri::command]
pub fn queue_watch_new(
    core: State<'_, Core_>,
    key: String,
    ids: Option<Vec<String>>,
) -> R<Option<String>> {
    let entry = core
        .watcher
        .list()
        .into_iter()
        .find(|e| e.key == key)
        .ok_or("not-found")?;
    let ids = ids.unwrap_or_else(|| entry.new_posts.iter().map(|p| p.id.clone()).collect());
    let job = core.watcher.queue(&entry, ids.clone());
    core.watcher.update(|entries| {
        if let Some(e) = entries.iter_mut().find(|e| e.key == key) {
            e.mark_seen(Some(&ids));
        }
    })?;
    Ok(job)
}

// ---------------------------------------------------------------- favorites

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FavoriteInput {
    service: String,
    creator_id: String,
    post_id: Option<String>,
    title: String,
    creator_name: Option<String>,
}

#[tauri::command]
pub fn list_favorites(core: State<'_, Core_>) -> Vec<Favorite> {
    core.store.list_favorites()
}

#[tauri::command]
pub fn add_favorite(core: State<'_, Core_>, input: FavoriteInput) -> R<Favorite> {
    let fav = match &input.post_id {
        Some(post_id) => Favorite::post(
            &input.service,
            &input.creator_id,
            post_id,
            &input.title,
            input.creator_name.as_deref(),
            now_ms(),
        ),
        None => Favorite::creator(&input.service, &input.creator_id, &input.title, now_ms()),
    };
    if let Some(existing) = core
        .store
        .list_favorites()
        .into_iter()
        .find(|f| f.key == fav.key)
    {
        return Ok(existing);
    }
    core.store.put_favorite(&fav)?;
    Ok(fav)
}

#[tauri::command]
pub fn update_favorite(core: State<'_, Core_>, key: String, title: String, note: String) -> R<()> {
    let mut fav = core
        .store
        .list_favorites()
        .into_iter()
        .find(|f| f.key == key)
        .ok_or("not-found")?;
    fav.title = title.trim().chars().take(300).collect();
    fav.note = note.chars().take(2000).collect();
    core.store.put_favorite(&fav)
}

#[tauri::command]
pub fn remove_favorites(core: State<'_, Core_>, keys: Vec<String>) -> R<()> {
    core.store.remove_favorites(&keys)
}

#[tauri::command]
pub fn queue_favorites(core: State<'_, Core_>, keys: Vec<String>) -> R<Vec<String>> {
    let favorites = core.store.list_favorites();
    let mut jobs = Vec::new();
    for mut fav in favorites.into_iter().filter(|f| keys.contains(&f.key)) {
        let id = match (&fav.kind, &fav.post_id) {
            (FavoriteKind::Post, Some(post_id)) => core.engine.create(JobRequest::Refs {
                title: Some(fav.title.clone()),
                creator: fav.creator_name.clone().map(|name| CreatorInfo {
                    service: fav.service.clone(),
                    id: fav.creator_id.clone(),
                    name,
                }),
                refs: vec![PostRef {
                    service: fav.service.clone(),
                    creator_id: fav.creator_id.clone(),
                    post_id: post_id.clone(),
                }],
                options: JobOverrides::default(),
            }),
            _ => core.engine.create(JobRequest::Creator {
                service: fav.service.clone(),
                creator_id: fav.creator_id.clone(),
                query: None,
                post_ids: None,
                since: None,
                until: None,
                options: JobOverrides::default(),
            }),
        };
        fav.last_downloaded_at = Some(now_ms());
        core.store.put_favorite(&fav)?;
        jobs.push(id);
    }
    Ok(jobs)
}

/// One link per line, with the title after a tab (KemonoDownloader's format).
#[tauri::command]
pub fn export_favorites(core: State<'_, Core_>, path: String) -> R<u32> {
    let favorites = core.store.list_favorites();
    let text: String = favorites
        .iter()
        .map(|f| format!("{}\t{}\n", f.url(), f.title.replace(['\t', '\n'], " ")))
        .collect();
    std::fs::write(path, text).map_err(|e| e.to_string())?;
    Ok(favorites.len() as u32)
}

#[tauri::command]
pub fn import_favorites(core: State<'_, Core_>, path: String) -> R<u32> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let existing: Vec<String> = core
        .store
        .list_favorites()
        .into_iter()
        .map(|f| f.key)
        .collect();
    let mut added = 0;
    for line in text.lines() {
        let (link, title) = line.split_once('\t').unwrap_or((line, ""));
        let fav = match parse_page_url(link) {
            PageRef::Post {
                service,
                creator_id,
                post_id,
            } => {
                let title = if title.trim().is_empty() {
                    post_id.clone()
                } else {
                    title.trim().to_string()
                };
                Favorite::post(&service, &creator_id, &post_id, &title, None, now_ms())
            }
            PageRef::Creator {
                service,
                creator_id,
                ..
            } => {
                let title = if title.trim().is_empty() {
                    creator_id.clone()
                } else {
                    title.trim().to_string()
                };
                Favorite::creator(&service, &creator_id, &title, now_ms())
            }
            _ => continue,
        };
        if existing.contains(&fav.key) {
            continue;
        }
        core.store.put_favorite(&fav)?;
        added += 1;
    }
    Ok(added)
}

// --------------------------------------------------------------------- logs

#[tauri::command]
pub fn get_logs(core: State<'_, Core_>) -> Vec<LogEntry> {
    core.log.entries()
}

#[tauri::command]
pub fn clear_logs(core: State<'_, Core_>) {
    core.log.clear();
}

#[tauri::command]
pub fn export_logs(core: State<'_, Core_>, path: String) -> R<()> {
    std::fs::write(path, core.log.export_text()).map_err(|e| e.to_string())
}

// -------------------------------------------------------------------- files

#[tauri::command]
pub fn open_path(app: AppHandle, path: String) -> R<()> {
    let p = PathBuf::from(&path);
    if !p.exists() {
        return Err("missing".into());
    }
    app.opener()
        .open_path(path, None::<&str>)
        .map_err(|e| e.to_string())
}

/// Shows a file in the file manager; falls back to the nearest folder that
/// still exists when it was moved or deleted.
#[tauri::command]
pub fn reveal_path(app: AppHandle, path: String) -> R<bool> {
    let p = PathBuf::from(&path);
    if p.exists() {
        return app
            .opener()
            .reveal_item_in_dir(&p)
            .map(|_| true)
            .map_err(|e| e.to_string());
    }
    let mut dir: Option<&Path> = p.parent();
    while let Some(d) = dir {
        if d.is_dir() {
            app.opener()
                .open_path(d.to_string_lossy(), None::<&str>)
                .map_err(|e| e.to_string())?;
            return Ok(false);
        }
        dir = d.parent();
    }
    Err("missing".into())
}

#[tauri::command]
pub fn path_exists(path: String) -> bool {
    Path::new(&path).exists()
}

#[tauri::command]
pub fn ensure_dir(path: String) -> R<()> {
    std::fs::create_dir_all(path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn write_text_file(path: String, text: String) -> R<()> {
    std::fs::write(path, text).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn read_text_file(path: String) -> R<String> {
    let meta = std::fs::metadata(&path).map_err(|e| e.to_string())?;
    if meta.len() > 8 * 1024 * 1024 {
        return Err("too-large".into());
    }
    std::fs::read_to_string(path).map_err(|e| e.to_string())
}

// ------------------------------------------------------------------ updates

#[tauri::command]
pub async fn check_updates(core: State<'_, Core_>) -> R<UpdateInfo> {
    Ok(updates::check(&core.client.http()).await)
}
