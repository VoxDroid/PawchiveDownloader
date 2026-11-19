//! Download jobs and their items. A job is what the user asked for (a post, a
//! selection, a whole creator); its items are the files, texts and archives
//! that make it up. Jobs are saved as JSON so they survive a crash.

use serde::{Deserialize, Serialize};

use super::filter::PostFilter;
use crate::pawchive::types::{FileCategory, Post};
use crate::settings::{ConflictAction, ContentFormat, DownloadMode, PreviewOnlyAction, Settings};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Resolving,
    Queued,
    Running,
    Paused,
    Done,
    Failed,
    Cancelled,
}

impl JobStatus {
    pub fn finished(self) -> bool {
        matches!(
            self,
            JobStatus::Done | JobStatus::Failed | JobStatus::Cancelled
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ItemStatus {
    Pending,
    Active,
    Done,
    Failed,
    Skipped,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SkipReason {
    Duplicate,
    PreviewOnly,
    NoRelay,
    Exists,
    /// Nothing to save (a post without comments).
    Empty,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobOptions {
    pub mode: DownloadMode,
    pub categories: Vec<FileCategory>,
    pub save_content: ContentFormat,
    pub save_metadata: bool,
    #[serde(default)]
    pub save_links: bool,
    #[serde(default)]
    pub save_comments: bool,
    #[serde(default)]
    pub save_announcements: bool,
    pub preview_only: PreviewOnlyAction,
    pub skip_downloaded: bool,
    pub verify_hashes: bool,
    pub thumbnail_fallback: bool,
    pub folder_template: String,
    pub file_template: String,
    pub archive_template: String,
    pub conflict_action: ConflictAction,
    pub zip_max_size_mb: u32,
    pub zip_videos_separately: bool,
    /// Where this job saves; fixed when the job is created.
    pub download_dir: String,
    #[serde(default)]
    pub filter: PostFilter,
    #[serde(default)]
    pub apply_post_filter: bool,
    /// Words taken out of post titles and file names.
    #[serde(default)]
    pub name_remove: Vec<String>,
}

/// Per-job changes to the settings, sent by the UI.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct JobOverrides {
    pub mode: Option<DownloadMode>,
    pub categories: Option<Vec<FileCategory>>,
    pub save_content: Option<ContentFormat>,
    pub save_metadata: Option<bool>,
    pub save_comments: Option<bool>,
    pub save_announcements: Option<bool>,
    pub preview_only: Option<PreviewOnlyAction>,
    pub skip_downloaded: Option<bool>,
    pub download_dir: Option<String>,
    /// Replaces the filter from the settings.
    pub filter: Option<PostFilter>,
    /// Apply the post rules of the filter to the posts of a list of links
    /// too (they otherwise apply to creator scans only).
    pub apply_post_filter: Option<bool>,
}

impl JobOptions {
    pub fn from_settings(s: &Settings, o: &JobOverrides) -> Self {
        let mut categories = o
            .categories
            .clone()
            .unwrap_or_else(|| s.enabled_categories());
        if s.fast_mode && o.categories.is_none() {
            categories = FileCategory::ALL.to_vec();
        }
        let download_dir = o
            .download_dir
            .clone()
            .filter(|d| std::path::Path::new(d).is_absolute())
            .unwrap_or_else(|| s.download_dir.clone());
        Self {
            mode: o.mode.unwrap_or(s.download_mode),
            categories,
            save_content: o.save_content.unwrap_or(s.save_content),
            save_metadata: o.save_metadata.unwrap_or(s.save_metadata),
            save_links: s.save_links,
            save_comments: o.save_comments.unwrap_or(s.save_comments),
            save_announcements: o.save_announcements.unwrap_or(s.save_announcements),
            preview_only: o.preview_only.unwrap_or(s.preview_only),
            skip_downloaded: o.skip_downloaded.unwrap_or(s.skip_downloaded),
            verify_hashes: s.verify_hashes,
            thumbnail_fallback: s.thumbnail_fallback,
            folder_template: s.folder_template.clone(),
            file_template: s.file_template.clone(),
            archive_template: s.archive_template.clone(),
            conflict_action: s.conflict_action,
            zip_max_size_mb: s.zip_max_size_mb,
            zip_videos_separately: s.zip_videos_separately,
            download_dir,
            filter: o.filter.as_ref().unwrap_or(&s.filter).cleaned(),
            apply_post_filter: o.apply_post_filter.unwrap_or(false),
            name_remove: s.name_remove.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostRef {
    pub service: String,
    pub creator_id: String,
    pub post_id: String,
}

impl PostRef {
    pub fn key(&self) -> String {
        format!("{}:{}:{}", self.service, self.creator_id, self.post_id)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatorInfo {
    pub service: String,
    pub id: String,
    pub name: String,
}

/// How to fetch one file's bytes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSource {
    /// Direct URL, when known up front.
    pub url: Option<String>,
    /// The full file has to come from the post page's temporary relay.
    pub relay: bool,
    /// Storage path, used to find the relay link on the post page.
    pub path: String,
    /// Original name, the fallback key for relay lookup.
    pub name: String,
    /// Thumbnail to save instead when the media server keeps failing.
    #[serde(default)]
    pub thumbnail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZipEntry {
    /// Path inside the archive.
    pub name: String,
    #[serde(default)]
    pub source: Option<FileSource>,
    #[serde(default)]
    pub text: Option<String>,
    pub hash: Option<String>,
    pub category: FileCategory,
    /// Fetched into the staging folder already (survives restarts).
    #[serde(default)]
    pub fetched: bool,
    #[serde(default)]
    pub failed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ItemKind {
    #[serde(rename_all = "camelCase")]
    File {
        source: FileSource,
        hash: Option<String>,
        category: FileCategory,
    },
    #[serde(rename_all = "camelCase")]
    Text { text: String },
    #[serde(rename_all = "camelCase")]
    Zip {
        entries: Vec<ZipEntry>,
        #[serde(default)]
        parts: u32,
    },
    /// The post's comments, fetched when the item runs.
    #[serde(rename_all = "camelCase")]
    Comments { title: String, url: String },
    /// The creator's announcements, fetched when the item runs.
    #[serde(rename_all = "camelCase")]
    Announcements { name: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: String,
    pub post: PostRef,
    /// Short label shown in the UI (file name).
    pub label: String,
    /// Target path relative to the job's download folder.
    pub path: String,
    /// The absolute file actually written, fixed at the first start so a
    /// resumed transfer keeps writing to the same file.
    #[serde(default)]
    pub target: Option<String>,
    pub status: ItemStatus,
    pub attempts: u32,
    pub bytes: u64,
    pub total_bytes: u64,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub skip_reason: Option<SkipReason>,
    /// Do not start again before this time (epoch ms), for retry backoff.
    #[serde(default)]
    pub not_before: Option<i64>,
    /// Saved as a thumbnail because the full file was not available.
    #[serde(default)]
    pub fallback: bool,
    /// Times this item waited for the network to come back.
    #[serde(default)]
    pub waits: u32,
    /// When it failed (epoch ms), for retrying it automatically later.
    #[serde(default)]
    pub failed_at: Option<i64>,
    /// Automatic retries it has had.
    #[serde(default)]
    pub auto_retries: u32,
    #[serde(flatten)]
    pub kind: ItemKind,
}

impl Item {
    pub fn is_open(&self) -> bool {
        matches!(self.status, ItemStatus::Pending | ItemStatus::Active)
    }
    pub fn hash(&self) -> Option<&str> {
        match &self.kind {
            ItemKind::File { hash, .. } => hash.as_deref(),
            _ => None,
        }
    }
    pub fn kind_name(&self) -> &'static str {
        match self.kind {
            ItemKind::File { .. } => "file",
            ItemKind::Text { .. } | ItemKind::Comments { .. } | ItemKind::Announcements { .. } => {
                "text"
            }
            ItemKind::Zip { .. } => "zip",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobStats {
    pub total: u32,
    pub done: u32,
    pub failed: u32,
    pub skipped: u32,
    pub cancelled: u32,
    pub active: u32,
    pub bytes: u64,
    pub total_bytes: u64,
    /// How far the running items are, summed as fractions (0..active).
    pub partial: f64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Scan {
    pub pages: u32,
    pub posts: u32,
    pub matched: u32,
}

/// What the UI sends to start a job.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum JobRequest {
    #[serde(rename_all = "camelCase")]
    Posts {
        #[serde(default)]
        title: Option<String>,
        #[serde(default)]
        creator: Option<CreatorInfo>,
        posts: Vec<SelectedPost>,
        #[serde(default)]
        options: JobOverrides,
    },
    #[serde(rename_all = "camelCase")]
    Refs {
        #[serde(default)]
        title: Option<String>,
        #[serde(default)]
        creator: Option<CreatorInfo>,
        refs: Vec<PostRef>,
        #[serde(default)]
        options: JobOverrides,
    },
    #[serde(rename_all = "camelCase")]
    Creator {
        service: String,
        creator_id: String,
        #[serde(default)]
        query: Option<String>,
        /// Only these posts; all of the creator's posts when omitted.
        #[serde(default)]
        post_ids: Option<Vec<String>>,
        /// Only posts published after this ISO date.
        #[serde(default)]
        since: Option<String>,
        /// Only posts published before this ISO date.
        #[serde(default)]
        until: Option<String>,
        #[serde(default)]
        options: JobOverrides,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectedPost {
    pub post: Post,
    #[serde(default)]
    pub file_indexes: Option<Vec<u32>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: String,
    pub created_at: i64,
    #[serde(default)]
    pub started_at: Option<i64>,
    #[serde(default)]
    pub finished_at: Option<i64>,
    pub title: String,
    pub subtitle: String,
    pub creator: Option<CreatorInfo>,
    pub status: JobStatus,
    pub options: JobOptions,
    /// Kept while the job still has to look up its posts.
    #[serde(default)]
    pub request: Option<JobRequest>,
    #[serde(default)]
    pub scan: Option<Scan>,
    pub post_count: u32,
    pub items: Vec<Item>,
    #[serde(default)]
    pub error: Option<String>,
    /// Posts to re-create this job later from history ("download again").
    pub refs: Vec<PostRef>,
    /// What was asked for, to notice the same download queued twice.
    #[serde(default)]
    pub signature: Option<String>,
}

/// The light version of a job sent to the UI several times a second.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobSummary {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub creator: Option<CreatorInfo>,
    pub status: JobStatus,
    pub mode: DownloadMode,
    pub created_at: i64,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub post_count: u32,
    pub stats: JobStats,
    pub scan: Option<Scan>,
    pub error: Option<String>,
    /// Bytes per second over the last few seconds.
    pub speed: f64,
    /// Labels of the items transferring right now.
    pub current: Vec<String>,
    pub folder: String,
}

/// One item as the job details view shows it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemView {
    pub id: String,
    pub kind: &'static str,
    pub label: String,
    pub path: String,
    pub target: Option<String>,
    pub status: ItemStatus,
    pub bytes: u64,
    pub total_bytes: u64,
    pub attempts: u32,
    pub error: Option<String>,
    pub skip_reason: Option<SkipReason>,
    pub fallback: bool,
    pub post_id: String,
    /// The fetched files are being written into the archive.
    pub building: bool,
}

pub fn compute_stats(items: &[Item]) -> JobStats {
    let mut s = JobStats {
        total: items.len() as u32,
        ..Default::default()
    };
    for item in items {
        match item.status {
            ItemStatus::Done => s.done += 1,
            ItemStatus::Failed => s.failed += 1,
            ItemStatus::Skipped => s.skipped += 1,
            ItemStatus::Cancelled => s.cancelled += 1,
            ItemStatus::Active => {
                s.active += 1;
                if item.total_bytes > 0 {
                    s.partial += (item.bytes as f64 / item.total_bytes as f64).min(1.0);
                }
            }
            ItemStatus::Pending => {}
        }
        s.bytes += item.bytes;
        s.total_bytes += item.total_bytes.max(item.bytes);
    }
    s
}
