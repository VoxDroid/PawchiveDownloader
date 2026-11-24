//! Records kept in the library: history entries, watched creators, favorites.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::jobs::model::{CreatorInfo, JobStatus, PostRef};
use crate::pawchive::types::Post;
use crate::settings::DownloadMode;

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileCounts {
    pub done: u32,
    pub failed: u32,
    pub skipped: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub creator: Option<CreatorInfo>,
    pub mode: DownloadMode,
    pub status: JobStatus,
    pub created_at: i64,
    pub started_at: Option<i64>,
    pub finished_at: i64,
    pub post_count: u32,
    pub files: FileCounts,
    pub bytes: u64,
    /// Absolute folder that holds the job's files.
    pub folder: String,
    /// One saved file, used to reveal the folder in the file manager.
    #[serde(default)]
    pub first_file: Option<String>,
    pub refs: Vec<PostRef>,
    #[serde(default)]
    pub errors: Vec<String>,
}

impl HistoryEntry {
    #[cfg(test)]
    pub fn sample(id: &str, finished_at: i64) -> Self {
        Self {
            id: id.into(),
            title: "T".into(),
            subtitle: String::new(),
            creator: None,
            mode: DownloadMode::Individual,
            status: JobStatus::Done,
            created_at: 0,
            started_at: None,
            finished_at,
            post_count: 1,
            files: FileCounts::default(),
            bytes: 0,
            folder: String::new(),
            first_file: None,
            refs: vec![],
            errors: vec![],
        }
    }
}

/// Rows for a CSV export of the history.
pub fn history_to_csv(entries: &[HistoryEntry]) -> String {
    let escape = |v: String| {
        if v.contains([',', '"', '\n', '\r']) {
            format!("\"{}\"", v.replace('"', "\"\""))
        } else {
            v
        }
    };
    let mut out = String::from(
        "finished,title,creator,service,status,mode,posts,files_done,files_failed,files_skipped,bytes,folder\r\n",
    );
    for e in entries {
        let finished = chrono::DateTime::from_timestamp_millis(e.finished_at)
            .map(|d| d.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
            .unwrap_or_default();
        let row = [
            finished,
            e.title.clone(),
            e.creator
                .as_ref()
                .map(|c| c.name.clone())
                .unwrap_or_default(),
            e.creator
                .as_ref()
                .map(|c| c.service.clone())
                .unwrap_or_default(),
            serde_json::to_value(e.status)
                .unwrap()
                .as_str()
                .unwrap_or("")
                .to_string(),
            serde_json::to_value(e.mode)
                .unwrap()
                .as_str()
                .unwrap_or("")
                .to_string(),
            e.post_count.to_string(),
            e.files.done.to_string(),
            e.files.failed.to_string(),
            e.files.skipped.to_string(),
            e.bytes.to_string(),
            e.folder.clone(),
        ];
        out.push_str(&row.into_iter().map(escape).collect::<Vec<_>>().join(","));
        out.push_str("\r\n");
    }
    out
}

pub const SEEN_LIMIT: usize = 300;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewPost {
    pub id: String,
    pub title: String,
    pub published: Option<String>,
    pub files: u32,
}

/// A creator the user follows for new posts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchEntry {
    pub key: String,
    pub service: String,
    pub creator_id: String,
    pub name: String,
    pub added_at: i64,
    pub last_checked_at: Option<i64>,
    /// Ids of the newest posts already seen (bounded).
    pub seen_ids: Vec<String>,
    pub new_posts: Vec<NewPost>,
    pub error: Option<String>,
    /// Queue new posts for download automatically when found.
    pub auto_download: bool,
    /// How this creator's new posts are downloaded.
    #[serde(default)]
    pub options: WatchOptions,
}

/// Per-creator choices for the watchlist; empty fields use the settings.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WatchOptions {
    /// Absolute folder for this creator's downloads.
    pub download_dir: Option<String>,
    pub mode: Option<crate::settings::DownloadMode>,
    /// Which new posts and files to take.
    pub filter: Option<crate::jobs::filter::PostFilter>,
}

pub fn watch_key(service: &str, creator_id: &str) -> String {
    format!("{service}:{creator_id}")
}

impl WatchEntry {
    pub fn new(service: &str, creator_id: &str, name: &str, now: i64) -> Self {
        Self {
            key: watch_key(service, creator_id),
            service: service.into(),
            creator_id: creator_id.into(),
            name: name.into(),
            added_at: now,
            last_checked_at: None,
            seen_ids: vec![],
            new_posts: vec![],
            error: None,
            auto_download: false,
            options: WatchOptions::default(),
        }
    }

    /// Applies a fresh first page of posts (newest first). The first check
    /// only records a baseline, so following a creator does not flag their
    /// whole back catalogue as new. Returns the posts found new this time.
    pub fn apply_check(&mut self, page: &[Post], now: i64) -> Vec<NewPost> {
        let seen: HashSet<&String> = self.seen_ids.iter().collect();
        // No successful check yet: never checked, or every check so far
        // failed (offline, the site down). A failed first check must not
        // turn the next good one into "the whole first page is new".
        let baseline =
            self.seen_ids.is_empty() && (self.last_checked_at.is_none() || self.error.is_some());
        let known: HashSet<String> = self.new_posts.iter().map(|p| p.id.clone()).collect();
        let found: Vec<NewPost> = if baseline {
            vec![]
        } else {
            page.iter()
                .filter(|p| !seen.contains(&p.id) && !known.contains(&p.id))
                .map(|p| NewPost {
                    id: p.id.clone(),
                    title: p.title.clone(),
                    published: p.published.clone(),
                    files: p.files.len() as u32,
                })
                .collect()
        };
        let mut new_posts = found.clone();
        new_posts.append(&mut self.new_posts);
        self.new_posts = new_posts;
        let mut ids: Vec<String> = Vec::new();
        let mut unique = HashSet::new();
        for id in page
            .iter()
            .map(|p| p.id.clone())
            .chain(self.seen_ids.drain(..))
        {
            if unique.insert(id.clone()) {
                ids.push(id);
            }
        }
        ids.truncate(SEEN_LIMIT);
        self.seen_ids = ids;
        self.last_checked_at = Some(now);
        self.error = None;
        found
    }

    pub fn mark_seen(&mut self, ids: Option<&[String]>) {
        match ids {
            None => self.new_posts.clear(),
            Some(ids) => self.new_posts.retain(|p| !ids.contains(&p.id)),
        }
    }
}

pub fn total_new(entries: &[WatchEntry]) -> usize {
    entries.iter().map(|e| e.new_posts.len()).sum()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FavoriteKind {
    Post,
    Creator,
}

/// A bookmarked post or creator.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Favorite {
    pub key: String,
    pub kind: FavoriteKind,
    pub service: String,
    pub creator_id: String,
    #[serde(default)]
    pub post_id: Option<String>,
    pub title: String,
    #[serde(default)]
    pub creator_name: Option<String>,
    #[serde(default)]
    pub note: String,
    pub added_at: i64,
    #[serde(default)]
    pub last_downloaded_at: Option<i64>,
}

impl Favorite {
    pub fn creator(service: &str, creator_id: &str, name: &str, now: i64) -> Self {
        Self {
            key: format!("c:{service}:{creator_id}"),
            kind: FavoriteKind::Creator,
            service: service.into(),
            creator_id: creator_id.into(),
            post_id: None,
            title: name.into(),
            creator_name: Some(name.into()),
            note: String::new(),
            added_at: now,
            last_downloaded_at: None,
        }
    }

    pub fn post(
        service: &str,
        creator_id: &str,
        post_id: &str,
        title: &str,
        creator_name: Option<&str>,
        now: i64,
    ) -> Self {
        Self {
            key: format!("p:{service}:{creator_id}:{post_id}"),
            kind: FavoriteKind::Post,
            service: service.into(),
            creator_id: creator_id.into(),
            post_id: Some(post_id.into()),
            title: title.into(),
            creator_name: creator_name.map(str::to_string),
            note: String::new(),
            added_at: now,
            last_downloaded_at: None,
        }
    }

    pub fn url(&self) -> String {
        match &self.post_id {
            Some(p) => crate::pawchive::urls::page::post(&self.service, &self.creator_id, p),
            None => crate::pawchive::urls::page::creator(&self.service, &self.creator_id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pawchive::normalize::normalize_post;

    fn posts(ids: &[&str]) -> Vec<Post> {
        ids.iter()
            .map(|id| normalize_post(serde_json::from_value(serde_json::json!({"id": id, "user": "1", "service": "s", "title": format!("P{id}")})).unwrap()))
            .collect()
    }

    #[test]
    fn first_check_is_a_baseline() {
        let mut w = WatchEntry::new("s", "1", "C", 0);
        assert!(w.apply_check(&posts(&["3", "2", "1"]), 10).is_empty());
        assert_eq!(w.seen_ids, ["3", "2", "1"]);
        let found = w.apply_check(&posts(&["5", "4", "3"]), 20);
        assert_eq!(
            found.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
            ["5", "4"]
        );
        assert_eq!(w.new_posts.len(), 2);
        // Seen again: not new twice.
        assert!(w.apply_check(&posts(&["5", "4"]), 30).is_empty());
        assert_eq!(w.seen_ids[0], "5");
        w.mark_seen(Some(&["5".to_string()]));
        assert_eq!(w.new_posts.len(), 1);
        assert_eq!(total_new(&[w.clone()]), 1);
        w.mark_seen(None);
        assert!(w.new_posts.is_empty());
    }

    #[test]
    fn a_failed_first_check_still_takes_a_baseline() {
        let mut w = WatchEntry::new("s", "1", "C", 0);
        // What a failed check leaves behind.
        w.error = Some("offline".into());
        w.last_checked_at = Some(5);
        assert!(w.apply_check(&posts(&["3", "2", "1"]), 10).is_empty());
        assert!(w.new_posts.is_empty());
        assert_eq!(w.apply_check(&posts(&["4", "3"]), 20).len(), 1);
        // A creator with no posts yet: their first post is new.
        let mut empty = WatchEntry::new("s", "2", "D", 0);
        assert!(empty.apply_check(&[], 10).is_empty());
        assert_eq!(empty.apply_check(&posts(&["1"]), 20).len(), 1);
    }

    #[test]
    fn exports_csv() {
        let mut e = HistoryEntry::sample("a", 0);
        e.title = "Hello, \"World\"".into();
        let csv = history_to_csv(&[e]);
        assert!(csv.starts_with("finished,title"));
        assert!(csv.contains("\"Hello, \"\"World\"\"\""));
        assert!(csv.contains(",done,individual,"));
    }

    #[test]
    fn favorites_have_keys_and_urls() {
        let f = Favorite::post("s", "1", "9", "T", None, 0);
        assert_eq!(f.key, "p:s:1:9");
        assert_eq!(f.url(), "https://pawchive.pw/s/user/1/post/9");
        assert_eq!(
            Favorite::creator("s", "1", "C", 0).url(),
            "https://pawchive.pw/s/user/1"
        );
    }
}
