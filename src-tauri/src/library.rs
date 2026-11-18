//! What the library knows about past downloads: lifetime statistics, and a
//! check that finds files deleted or moved since they were downloaded so
//! they can be fetched again.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use chrono::{Datelike, TimeZone};
use serde::{Deserialize, Serialize};

use crate::pawchive::normalize::{category_of, extension_of};
use crate::pawchive::types::FileCategory;
use crate::store::records::HistoryEntry;
use crate::store::{KnownFile, Store};

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatorStat {
    pub service: String,
    pub id: String,
    pub name: String,
    pub files: u64,
    pub bytes: u64,
    pub downloads: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonthStat {
    /// `YYYY-MM`, local time.
    pub month: String,
    pub files: u64,
    pub bytes: u64,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    /// Finished downloads in the history.
    pub downloads: u64,
    pub files: u64,
    pub failed: u64,
    pub skipped: u64,
    pub bytes: u64,
    pub posts: u64,
    pub creators: u64,
    /// Files the library remembers (for skipping duplicates).
    pub known_files: u64,
    pub first_at: Option<i64>,
    pub last_at: Option<i64>,
    /// Most downloaded creators, by bytes.
    pub top_creators: Vec<CreatorStat>,
    /// The last twelve months, oldest first.
    pub months: Vec<MonthStat>,
    /// Remembered files by kind.
    pub categories: BTreeMap<FileCategory, u64>,
}

fn month_key(ms: i64) -> String {
    match chrono::Local.timestamp_millis_opt(ms).single() {
        Some(d) => format!("{:04}-{:02}", d.year(), d.month()),
        None => String::new(),
    }
}

/// The twelve months up to and including the one holding `now_ms`.
fn last_twelve_months(now_ms: i64) -> Vec<String> {
    let Some(now) = chrono::Local.timestamp_millis_opt(now_ms).single() else {
        return vec![];
    };
    let (mut y, mut m) = (now.year(), now.month() as i32);
    let mut out = Vec::with_capacity(12);
    for _ in 0..12 {
        out.push(format!("{y:04}-{m:02}"));
        m -= 1;
        if m == 0 {
            m = 12;
            y -= 1;
        }
    }
    out.reverse();
    out
}

pub fn stats(history: &[HistoryEntry], known: &[KnownFile], now_ms: i64) -> Stats {
    let mut s = Stats {
        downloads: history.len() as u64,
        known_files: known.len() as u64,
        ..Default::default()
    };
    let mut creators: HashMap<String, CreatorStat> = HashMap::new();
    let months = last_twelve_months(now_ms);
    let mut by_month: HashMap<String, (u64, u64)> = HashMap::new();
    let mut posts = HashSet::new();
    for h in history {
        s.files += u64::from(h.files.done);
        s.failed += u64::from(h.files.failed);
        s.skipped += u64::from(h.files.skipped);
        s.bytes += h.bytes;
        s.first_at = Some(s.first_at.map_or(h.finished_at, |f| f.min(h.finished_at)));
        s.last_at = Some(s.last_at.map_or(h.finished_at, |l| l.max(h.finished_at)));
        for r in &h.refs {
            posts.insert(r.key());
        }
        let month = by_month.entry(month_key(h.finished_at)).or_default();
        month.0 += u64::from(h.files.done);
        month.1 += h.bytes;
        let (service, id, name) = match &h.creator {
            Some(c) => (c.service.clone(), c.id.clone(), c.name.clone()),
            None => match h.refs.first() {
                Some(r) if h.refs.iter().all(|x| x.creator_id == r.creator_id) => (
                    r.service.clone(),
                    r.creator_id.clone(),
                    r.creator_id.clone(),
                ),
                _ => continue,
            },
        };
        let c = creators
            .entry(format!("{service}:{id}"))
            .or_insert_with(|| CreatorStat {
                service,
                id,
                name: name.clone(),
                ..Default::default()
            });
        // Prefer a real name over the id a link-only download had.
        if c.name == c.id && name != c.id {
            c.name = name;
        }
        c.files += u64::from(h.files.done);
        c.bytes += h.bytes;
        c.downloads += 1;
    }
    s.posts = posts.len() as u64;
    s.creators = creators.len() as u64;
    let mut top: Vec<CreatorStat> = creators.into_values().collect();
    top.sort_by(|a, b| b.bytes.cmp(&a.bytes).then(b.files.cmp(&a.files)));
    top.truncate(10);
    s.top_creators = top;
    s.months = months
        .into_iter()
        .map(|month| {
            let (files, bytes) = by_month.get(&month).copied().unwrap_or_default();
            MonthStat {
                month,
                files,
                bytes,
            }
        })
        .collect();
    for f in known {
        let ext = extension_of(&f.path);
        // Files inside a ZIP are recorded with the archive's path.
        let category = if ext == "zip" && f.path.ends_with(".zip") {
            FileCategory::Archive
        } else {
            category_of(&ext)
        };
        *s.categories.entry(category).or_default() += 1;
    }
    s
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MissingPost {
    /// `service:creator:post`.
    pub post: String,
    pub hashes: Vec<String>,
    /// One of the missing paths, to show.
    #[serde(default)]
    pub example: String,
    #[serde(default)]
    pub count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryCheck {
    pub checked: usize,
    pub missing_files: usize,
    pub posts: Vec<MissingPost>,
}

/// Looks for remembered files that are no longer where they were saved.
/// Blocking: run it off the async threads.
pub fn check(known: &[KnownFile]) -> LibraryCheck {
    let mut exists: HashMap<&str, bool> = HashMap::new();
    let mut posts: BTreeMap<String, MissingPost> = BTreeMap::new();
    let mut missing_files = 0;
    for f in known {
        let present = *exists
            .entry(f.path.as_str())
            .or_insert_with(|| Path::new(&f.path).is_file());
        if present {
            continue;
        }
        missing_files += 1;
        let entry = posts.entry(f.post.clone()).or_insert_with(|| MissingPost {
            post: f.post.clone(),
            hashes: vec![],
            example: f.path.clone(),
            count: 0,
        });
        entry.hashes.push(f.hash.clone());
        entry.count += 1;
    }
    LibraryCheck {
        checked: known.len(),
        missing_files,
        posts: posts.into_values().collect(),
    }
}

/// Forgets missing files (and their posts' "downloaded" marks) so the next
/// download fetches them again.
pub fn forget(store: &Store, posts: &[MissingPost]) -> Result<(), String> {
    let hashes: Vec<String> = posts.iter().flat_map(|p| p.hashes.clone()).collect();
    let keys: Vec<String> = posts.iter().map(|p| p.post.clone()).collect();
    store.forget_files(&hashes, &keys)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::model::{CreatorInfo, PostRef};

    fn entry(
        id: &str,
        at: i64,
        creator: Option<(&str, &str)>,
        files: u32,
        bytes: u64,
    ) -> HistoryEntry {
        let mut h = HistoryEntry::sample(id, at);
        h.creator = creator.map(|(id, name)| CreatorInfo {
            service: "fanbox".into(),
            id: id.into(),
            name: name.into(),
        });
        h.files.done = files;
        h.bytes = bytes;
        h.refs = vec![PostRef {
            service: "fanbox".into(),
            creator_id: creator.map(|c| c.0).unwrap_or("9").into(),
            post_id: id.into(),
        }];
        h
    }

    #[test]
    fn sums_history_by_creator_and_month() {
        let now = chrono::Local
            .with_ymd_and_hms(2026, 11, 18, 12, 0, 0)
            .unwrap()
            .timestamp_millis();
        let older = chrono::Local
            .with_ymd_and_hms(2026, 3, 2, 12, 0, 0)
            .unwrap()
            .timestamp_millis();
        let history = vec![
            entry("1", now, Some(("5", "Cat")), 3, 300),
            entry("2", older, Some(("5", "Cat")), 2, 200),
            entry("3", now, Some(("6", "Dog")), 9, 900),
            entry("4", now, None, 1, 10),
        ];
        let known = vec![
            KnownFile {
                hash: "a".into(),
                post: "p".into(),
                path: "/x/a.PNG".into(),
            },
            KnownFile {
                hash: "b".into(),
                post: "p".into(),
                path: "/x/b.zip".into(),
            },
        ];
        let s = stats(&history, &known, now);
        assert_eq!(
            (s.downloads, s.files, s.bytes, s.posts, s.creators),
            (4, 15, 1410, 4, 3)
        );
        assert_eq!(s.top_creators[0].name, "Dog");
        assert_eq!(s.top_creators[1].files, 5);
        assert_eq!(s.months.len(), 12);
        assert_eq!(s.months[11].month, "2026-11");
        assert_eq!(s.months[11].files, 13);
        assert_eq!(s.months[3].month, "2026-03");
        assert_eq!(s.months[3].bytes, 200);
        assert_eq!(s.categories[&FileCategory::Image], 1);
        assert_eq!(s.categories[&FileCategory::Archive], 1);
        assert_eq!(s.first_at, Some(older));
    }

    #[test]
    fn finds_missing_files_grouped_by_post() {
        let dir = tempfile::tempdir().unwrap();
        let kept = dir.path().join("kept.png");
        std::fs::write(&kept, b"x").unwrap();
        let gone = dir.path().join("gone.png");
        let known = vec![
            KnownFile {
                hash: "1".into(),
                post: "s:c:1".into(),
                path: kept.to_string_lossy().into(),
            },
            KnownFile {
                hash: "2".into(),
                post: "s:c:1".into(),
                path: gone.to_string_lossy().into(),
            },
            KnownFile {
                hash: "3".into(),
                post: "s:c:2".into(),
                path: gone.to_string_lossy().into(),
            },
        ];
        let r = check(&known);
        assert_eq!((r.checked, r.missing_files, r.posts.len()), (3, 2, 2));
        assert_eq!(r.posts[0].hashes, ["2"]);
    }
}
