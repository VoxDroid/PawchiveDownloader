//! User settings. Stored as JSON; anything unknown or out of range falls back
//! to its default so an old or hand-edited file can never break the app.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::jobs::filter::{clean_terms, PostFilter};
use crate::naming::template::{
    validate_template, DEFAULT_ARCHIVE_TEMPLATE, DEFAULT_FILE_TEMPLATE, DEFAULT_FOLDER_TEMPLATE,
};
use crate::net::http::{ProxyMode, ProxySettings};
use crate::pawchive::types::FileCategory;

pub const LOCALES: [&str; 9] = ["en", "ja", "ko", "zh_CN", "es", "pt_BR", "ru", "de", "fr"];
pub const THEMES: [&str; 6] = ["midnight", "graphite", "plum", "forest", "ember", "light"];
/// Pawchive asks for at most five downloads at once...
pub const MAX_CONCURRENCY: u32 = 5;
/// ...and at most one new request a second.
pub const MIN_REQUEST_INTERVAL_MS: u32 = 1000;
pub const WATCH_INTERVALS: [u32; 6] = [0, 60, 180, 360, 720, 1440];
/// Choices for the free space the download drive must keep (MB; 0 is off).
pub const FREE_SPACE_LIMITS: [u32; 6] = [0, 512, 1024, 2048, 5120, 10240];

/// `HH:MM` in 24-hour time, or None.
pub fn parse_clock(value: &str) -> Option<(u32, u32)> {
    let (h, m) = value.trim().split_once(':')?;
    let (h, m) = (h.parse::<u32>().ok()?, m.parse::<u32>().ok()?);
    (h < 24 && m < 60).then_some((h, m))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DownloadMode {
    Individual,
    Zip,
    /// Only the thumbnails and post text, for when the media servers are down.
    Thumbnails,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConflictAction {
    Uniquify,
    Overwrite,
    Skip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PreviewOnlyAction {
    Relay,
    Thumbnail,
    Skip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContentFormat {
    None,
    Txt,
    Html,
    Both,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    // Appearance
    pub language: String,
    pub theme: String,
    pub accent: String,
    pub reduce_motion: String,
    // Downloads
    pub download_dir: String,
    pub download_mode: DownloadMode,
    pub concurrency: u32,
    pub request_interval_ms: u32,
    pub max_retries: u32,
    pub folder_template: String,
    pub file_template: String,
    pub archive_template: String,
    pub conflict_action: ConflictAction,
    pub skip_downloaded: bool,
    pub verify_hashes: bool,
    pub preview_only: PreviewOnlyAction,
    /// Save the thumbnail when a file's media server keeps failing.
    pub thumbnail_fallback: bool,
    pub save_content: ContentFormat,
    pub save_metadata: bool,
    /// Save links to outside file hosts and passwords found in posts.
    pub save_links: bool,
    pub save_comments: bool,
    /// Creator downloads also save the creator's announcements.
    pub save_announcements: bool,
    pub categories: BTreeMap<FileCategory, bool>,
    /// Bytes per second over all transfers; 0 is unlimited.
    pub speed_limit_kbps: u32,
    /// Keep the computer from sleeping while downloads run.
    pub keep_awake: bool,
    /// Hold new downloads when the drive has less than this free (MB, 0 off).
    pub min_free_space_mb: u32,
    /// Only start downloads between `schedule_start` and `schedule_end`.
    pub schedule_enabled: bool,
    pub schedule_start: String,
    pub schedule_end: String,
    /// One click: every category, no prompts, batch input on.
    pub fast_mode: bool,
    // ZIP
    /// Split archives above this size; 0 keeps every post in one archive.
    pub zip_max_size_mb: u32,
    pub zip_videos_separately: bool,
    // Filters
    pub filter: PostFilter,
    /// Words taken out of post titles and file names.
    pub name_remove: Vec<String>,
    // Network
    pub proxy: ProxySettings,
    // Background
    pub notifications: String,
    pub watch_interval_minutes: u32,
    pub record_history: bool,
    pub log_level: String,
    pub close_to_tray: bool,
    pub start_minimized: bool,
    pub launch_at_login: bool,
    pub check_updates: bool,
    pub confirm_large_jobs: bool,
    pub onboarded: bool,
}

pub const ACCENTS: [&str; 6] = [
    "#8b7cf6", "#5aa9ff", "#3ccf91", "#f5a524", "#ff6b8b", "#22d3ee",
];

pub fn default_download_dir() -> String {
    dirs::download_dir()
        .or_else(dirs::home_dir)
        .map(|d| d.join("Pawchive").to_string_lossy().into_owned())
        .unwrap_or_else(|| "Pawchive".into())
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: "auto".into(),
            theme: "midnight".into(),
            accent: ACCENTS[0].into(),
            reduce_motion: "auto".into(),
            download_dir: default_download_dir(),
            download_mode: DownloadMode::Individual,
            concurrency: 3,
            request_interval_ms: 1000,
            max_retries: 5,
            folder_template: DEFAULT_FOLDER_TEMPLATE.into(),
            file_template: DEFAULT_FILE_TEMPLATE.into(),
            archive_template: DEFAULT_ARCHIVE_TEMPLATE.into(),
            conflict_action: ConflictAction::Uniquify,
            skip_downloaded: true,
            verify_hashes: true,
            preview_only: PreviewOnlyAction::Relay,
            thumbnail_fallback: false,
            save_content: ContentFormat::Txt,
            save_metadata: false,
            save_links: true,
            save_comments: false,
            save_announcements: false,
            categories: FileCategory::ALL.iter().map(|c| (*c, true)).collect(),
            speed_limit_kbps: 0,
            keep_awake: true,
            min_free_space_mb: 1024,
            schedule_enabled: false,
            schedule_start: "01:00".into(),
            schedule_end: "07:00".into(),
            fast_mode: false,
            zip_max_size_mb: 0,
            zip_videos_separately: false,
            filter: PostFilter::default(),
            name_remove: Vec::new(),
            proxy: ProxySettings {
                mode: ProxyMode::System,
                url: String::new(),
            },
            notifications: "all".into(),
            watch_interval_minutes: 360,
            record_history: true,
            log_level: "info".into(),
            close_to_tray: true,
            start_minimized: false,
            launch_at_login: false,
            check_updates: true,
            confirm_large_jobs: true,
            onboarded: false,
        }
    }
}

fn one_of(value: Option<&Value>, options: &[&str], fallback: &str) -> String {
    value
        .and_then(Value::as_str)
        .filter(|v| options.contains(v))
        .unwrap_or(fallback)
        .to_string()
}

fn int_in(value: Option<&Value>, min: u32, max: u32, fallback: u32) -> u32 {
    let n = match value {
        Some(Value::Number(n)) => n.as_f64(),
        Some(Value::String(s)) => s.trim().parse::<f64>().ok(),
        _ => None,
    };
    match n {
        Some(n) if n.is_finite() => (n.round().max(min as f64).min(max as f64)) as u32,
        _ => fallback,
    }
}

fn boolean(value: Option<&Value>, fallback: bool) -> bool {
    value.and_then(Value::as_bool).unwrap_or(fallback)
}

fn clock(value: Option<&Value>, fallback: &str) -> String {
    match value.and_then(Value::as_str).and_then(parse_clock) {
        Some((h, m)) => format!("{h:02}:{m:02}"),
        None => fallback.to_string(),
    }
}

fn enum_of<T: for<'de> Deserialize<'de>>(value: Option<&Value>, fallback: T) -> T {
    value
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or(fallback)
}

fn template(value: Option<&Value>, kind: &str, fallback: &str) -> String {
    match value.and_then(Value::as_str) {
        Some(t) => {
            let t: String = t.trim().chars().take(300).collect();
            if validate_template(&t, kind).is_none() {
                t
            } else {
                fallback.to_string()
            }
        }
        None => fallback.to_string(),
    }
}

/// Builds valid settings from anything (stored data from an older version, an
/// imported file, a partial update).
pub fn coerce(input: &Value) -> Settings {
    let d = Settings::default();
    let empty = serde_json::Map::new();
    let s = input.as_object().unwrap_or(&empty);
    let g = |k: &str| s.get(k);
    let mut categories = d.categories.clone();
    if let Some(Value::Object(map)) = g("categories") {
        for c in FileCategory::ALL {
            let key = serde_json::to_value(c).unwrap();
            if let Some(Value::Bool(on)) = map.get(key.as_str().unwrap()) {
                categories.insert(c, *on);
            }
        }
    }
    let mut proxy = d.proxy.clone();
    if let Some(Value::Object(p)) = g("proxy") {
        proxy.mode = enum_of(p.get("mode"), proxy.mode);
        proxy.url = p
            .get("url")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .chars()
            .take(500)
            .collect();
    }
    let download_dir = g("downloadDir")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|p| !p.is_empty() && std::path::Path::new(p).is_absolute())
        .map(str::to_string)
        .unwrap_or(d.download_dir.clone());
    let accent = g("accent")
        .and_then(Value::as_str)
        .filter(|a| {
            a.len() == 7 && a.starts_with('#') && a[1..].chars().all(|c| c.is_ascii_hexdigit())
        })
        .map(str::to_ascii_lowercase)
        .unwrap_or(d.accent.clone());
    let mut languages = vec!["auto"];
    languages.extend(LOCALES);
    let mut themes = vec!["system"];
    themes.extend(THEMES);
    Settings {
        language: one_of(g("language"), &languages, &d.language),
        theme: one_of(g("theme"), &themes, &d.theme),
        accent,
        reduce_motion: one_of(g("reduceMotion"), &["auto", "on", "off"], &d.reduce_motion),
        download_dir,
        download_mode: enum_of(g("downloadMode"), d.download_mode),
        concurrency: int_in(g("concurrency"), 1, MAX_CONCURRENCY, d.concurrency),
        request_interval_ms: int_in(
            g("requestIntervalMs"),
            MIN_REQUEST_INTERVAL_MS,
            10_000,
            d.request_interval_ms,
        ),
        max_retries: int_in(g("maxRetries"), 0, 50, d.max_retries),
        folder_template: template(g("folderTemplate"), "folder", &d.folder_template),
        file_template: template(g("fileTemplate"), "file", &d.file_template),
        archive_template: template(g("archiveTemplate"), "file", &d.archive_template),
        conflict_action: enum_of(g("conflictAction"), d.conflict_action),
        skip_downloaded: boolean(g("skipDownloaded"), d.skip_downloaded),
        verify_hashes: boolean(g("verifyHashes"), d.verify_hashes),
        preview_only: enum_of(g("previewOnly"), d.preview_only),
        thumbnail_fallback: boolean(g("thumbnailFallback"), d.thumbnail_fallback),
        save_content: enum_of(g("saveContent"), d.save_content),
        save_metadata: boolean(g("saveMetadata"), d.save_metadata),
        save_links: boolean(g("saveLinks"), d.save_links),
        save_comments: boolean(g("saveComments"), d.save_comments),
        save_announcements: boolean(g("saveAnnouncements"), d.save_announcements),
        categories,
        speed_limit_kbps: int_in(g("speedLimitKbps"), 0, 1_000_000, d.speed_limit_kbps),
        keep_awake: boolean(g("keepAwake"), d.keep_awake),
        min_free_space_mb: g("minFreeSpaceMb")
            .and_then(Value::as_u64)
            .map(|v| v as u32)
            .filter(|v| FREE_SPACE_LIMITS.contains(v))
            .unwrap_or(d.min_free_space_mb),
        schedule_enabled: boolean(g("scheduleEnabled"), d.schedule_enabled),
        schedule_start: clock(g("scheduleStart"), &d.schedule_start),
        schedule_end: clock(g("scheduleEnd"), &d.schedule_end),
        fast_mode: boolean(g("fastMode"), d.fast_mode),
        zip_max_size_mb: int_in(g("zipMaxSizeMb"), 0, 1_000_000, d.zip_max_size_mb),
        zip_videos_separately: boolean(g("zipVideosSeparately"), d.zip_videos_separately),
        filter: g("filter")
            .and_then(|v| serde_json::from_value::<PostFilter>(v.clone()).ok())
            .map(|f| f.cleaned())
            .unwrap_or_default(),
        name_remove: g("nameRemove")
            .and_then(|v| serde_json::from_value::<Vec<String>>(v.clone()).ok())
            .map(|v| clean_terms(&v))
            .unwrap_or_default(),
        proxy,
        notifications: one_of(
            g("notifications"),
            &["all", "errors", "none"],
            &d.notifications,
        ),
        watch_interval_minutes: g("watchIntervalMinutes")
            .and_then(Value::as_u64)
            .map(|v| v as u32)
            .filter(|v| WATCH_INTERVALS.contains(v))
            .unwrap_or(d.watch_interval_minutes),
        record_history: boolean(g("recordHistory"), d.record_history),
        log_level: one_of(
            g("logLevel"),
            &["debug", "info", "warn", "error"],
            &d.log_level,
        ),
        close_to_tray: boolean(g("closeToTray"), d.close_to_tray),
        start_minimized: boolean(g("startMinimized"), d.start_minimized),
        launch_at_login: boolean(g("launchAtLogin"), d.launch_at_login),
        check_updates: boolean(g("checkUpdates"), d.check_updates),
        confirm_large_jobs: boolean(g("confirmLargeJobs"), d.confirm_large_jobs),
        onboarded: boolean(g("onboarded"), d.onboarded),
    }
}

/// Applies a partial update (only the keys present) on top of `current`.
pub fn merge(current: &Settings, patch: &Value) -> Settings {
    let mut base = serde_json::to_value(current).unwrap();
    if let (Some(base), Some(patch)) = (base.as_object_mut(), patch.as_object()) {
        for (k, v) in patch {
            if k == "categories" || k == "proxy" {
                if let (Some(Value::Object(dst)), Value::Object(src)) = (base.get_mut(k), v) {
                    for (kk, vv) in src {
                        dst.insert(kk.clone(), vv.clone());
                    }
                    continue;
                }
            }
            base.insert(k.clone(), v.clone());
        }
    }
    coerce(&base)
}

impl Settings {
    pub fn enabled_categories(&self) -> Vec<FileCategory> {
        FileCategory::ALL
            .into_iter()
            .filter(|c| self.categories.get(c).copied().unwrap_or(true))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn defaults_round_trip() {
        let d = Settings::default();
        assert_eq!(coerce(&serde_json::to_value(&d).unwrap()), d);
        assert_eq!(coerce(&json!(null)), d);
    }

    #[test]
    fn clamps_and_rejects_bad_values() {
        let s = coerce(&json!({
            "concurrency": 50, "requestIntervalMs": 10, "maxRetries": "7", "theme": "neon",
            "language": "ja", "accent": "#ABCDEF", "downloadMode": "zip", "conflictAction": "prompt",
            "folderTemplate": "../x", "fileTemplate": "{bad}", "categories": {"video": false, "nope": true},
            "downloadDir": "relative/path", "watchIntervalMinutes": 5, "proxy": {"mode": "tor"}
        }));
        assert_eq!(s.concurrency, MAX_CONCURRENCY);
        assert_eq!(s.request_interval_ms, MIN_REQUEST_INTERVAL_MS);
        assert_eq!(s.max_retries, 7);
        assert_eq!(s.theme, "midnight");
        assert_eq!(s.language, "ja");
        assert_eq!(s.accent, "#abcdef");
        assert_eq!(s.download_mode, DownloadMode::Zip);
        assert_eq!(s.conflict_action, ConflictAction::Uniquify);
        assert_eq!(s.folder_template, DEFAULT_FOLDER_TEMPLATE);
        assert_eq!(s.file_template, DEFAULT_FILE_TEMPLATE);
        assert!(!s.categories[&FileCategory::Video]);
        assert!(!s.enabled_categories().contains(&FileCategory::Video));
        assert_eq!(s.download_dir, default_download_dir());
        assert_eq!(s.watch_interval_minutes, 360);
        assert_eq!(s.proxy.mode, ProxyMode::Tor);
        let s =
            coerce(&json!({"scheduleStart": "9:5", "scheduleEnd": "24:00", "minFreeSpaceMb": 7}));
        assert_eq!(s.schedule_start, "09:05");
        assert_eq!(s.schedule_end, "07:00");
        assert_eq!(s.min_free_space_mb, 1024);
    }

    #[test]
    fn merges_partial_updates() {
        let d = Settings::default();
        let s = merge(
            &d,
            &json!({"categories": {"image": false}, "proxy": {"url": "socks5://h:1"}, "theme": "plum"}),
        );
        assert!(!s.categories[&FileCategory::Image]);
        assert!(s.categories[&FileCategory::Video]);
        assert_eq!(s.proxy.url, "socks5://h:1");
        assert_eq!(s.proxy.mode, ProxyMode::System);
        assert_eq!(s.theme, "plum");
    }
}
