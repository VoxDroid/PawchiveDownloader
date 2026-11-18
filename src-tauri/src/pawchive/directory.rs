//! The list of every creator Pawchive has, for searching creators by name.
//!
//! The site publishes it as one large JSON file (about 100,000 creators, 3 MB
//! compressed). It is fetched at most once a day, kept on disk, and searched
//! in memory.

use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::RwLock;
use serde::{Deserialize, Deserializer, Serialize};
use tokio_util::sync::CancellationToken;

use super::client::{ApiError, PawchiveClient};
use super::urls::page;

/// How long a fetched list is used before it is fetched again.
pub const MAX_AGE_MS: i64 = 24 * 60 * 60 * 1000;
pub const MAX_RESULTS: usize = 200;

fn string_or_number<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    Ok(match serde_json::Value::deserialize(d)? {
        serde_json::Value::String(s) => s,
        serde_json::Value::Number(n) => n.to_string(),
        _ => String::new(),
    })
}

fn number_or_zero<'de, D: Deserializer<'de>>(d: D) -> Result<i64, D::Error> {
    Ok(match serde_json::Value::deserialize(d)? {
        serde_json::Value::Number(n) => n.as_i64().unwrap_or(0),
        serde_json::Value::String(s) => s.parse().unwrap_or(0),
        _ => 0,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    #[serde(deserialize_with = "string_or_number")]
    pub id: String,
    #[serde(default, deserialize_with = "string_or_number")]
    pub name: String,
    pub service: String,
    #[serde(default)]
    pub public_id: Option<String>,
    /// How many Pawchive users favorited the creator.
    #[serde(default, deserialize_with = "number_or_zero")]
    pub favorited: i64,
    /// Unix seconds of the newest import.
    #[serde(default, deserialize_with = "number_or_zero")]
    pub updated: i64,
}

#[derive(Serialize, Deserialize)]
struct Cache {
    fetched_at: i64,
    entries: Vec<Entry>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hit {
    pub id: String,
    pub name: String,
    pub service: String,
    pub public_id: Option<String>,
    pub favorited: i64,
    /// Epoch milliseconds of the newest import, 0 when unknown.
    pub updated: i64,
    pub url: String,
    pub icon_url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub creators: Vec<Hit>,
    /// Matches before the result limit.
    pub total: usize,
    /// Creators in the whole list.
    pub size: usize,
    /// When the list was fetched (epoch ms).
    pub fetched_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Sort {
    /// Best match, then most favorited.
    #[default]
    Relevance,
    Favorited,
    Updated,
    Name,
}

struct Loaded {
    fetched_at: i64,
    entries: Arc<Vec<Entry>>,
    /// Lower-case names, searched instead of lower-casing on every query.
    folded: Arc<Vec<String>>,
}

pub struct Directory {
    path: PathBuf,
    loaded: RwLock<Option<Loaded>>,
    fetching: tokio::sync::Mutex<()>,
}

/// Lower case and without the spaces and marks people leave out when typing.
fn fold(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_whitespace() && !matches!(c, '_' | '-' | '.' | '\u{30fb}'))
        .flat_map(char::to_lowercase)
        .collect()
}

fn rank(name_folded: &str, entry: &Entry, query: &str) -> Option<u8> {
    if query.is_empty() {
        return Some(3);
    }
    if entry.id == query
        || entry.public_id.as_deref().is_some_and(|p| fold(p) == query)
        || name_folded == query
    {
        return Some(0);
    }
    if name_folded.starts_with(query) {
        return Some(1);
    }
    if name_folded.contains(query) {
        return Some(2);
    }
    if entry
        .public_id
        .as_deref()
        .is_some_and(|p| fold(p).contains(query))
    {
        return Some(2);
    }
    None
}

impl Directory {
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            path: data_dir.join("creators.json"),
            loaded: RwLock::new(None),
            fetching: tokio::sync::Mutex::new(()),
        }
    }

    fn set(&self, fetched_at: i64, entries: Vec<Entry>) {
        let folded = entries.iter().map(|e| fold(&e.name)).collect();
        *self.loaded.write() = Some(Loaded {
            fetched_at,
            entries: Arc::new(entries),
            folded: Arc::new(folded),
        });
    }

    fn read_disk(&self) -> Option<Cache> {
        let bytes = std::fs::read(&self.path).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    fn fetched_at(&self) -> Option<i64> {
        self.loaded.read().as_ref().map(|l| l.fetched_at)
    }

    /// Makes sure a list is loaded, fetching a new one when there is none or
    /// it is older than a day (or when `force`). An old list is still used
    /// when the site cannot be reached.
    pub async fn ensure(
        &self,
        client: &PawchiveClient,
        now_ms: i64,
        force: bool,
        cancel: &CancellationToken,
    ) -> Result<(), ApiError> {
        let fresh = |at: Option<i64>| at.is_some_and(|at| now_ms - at < MAX_AGE_MS);
        if !force && fresh(self.fetched_at()) {
            return Ok(());
        }
        let _guard = self.fetching.lock().await;
        if self.fetched_at().is_none() {
            if let Some(cache) = self.read_disk() {
                self.set(cache.fetched_at, cache.entries);
            }
        }
        if !force && fresh(self.fetched_at()) {
            return Ok(());
        }
        match client.get_creator_list(cancel).await {
            Ok(entries) if !entries.is_empty() => {
                let cache = Cache {
                    fetched_at: now_ms,
                    entries,
                };
                if let Ok(json) = serde_json::to_vec(&cache) {
                    if let Some(dir) = self.path.parent() {
                        let _ = std::fs::create_dir_all(dir);
                    }
                    let tmp = self.path.with_extension("json.tmp");
                    if std::fs::write(&tmp, json).is_ok() {
                        let _ = std::fs::rename(&tmp, &self.path);
                    }
                }
                self.set(cache.fetched_at, cache.entries);
                Ok(())
            }
            Ok(_) if self.fetched_at().is_some() => Ok(()),
            Ok(_) => Err(ApiError::Format {
                url: "creators".into(),
            }),
            Err(e) if self.fetched_at().is_some() && !e.is_cancelled() => Ok(()),
            Err(e) => Err(e),
        }
    }

    pub fn search(
        &self,
        query: &str,
        service: Option<&str>,
        sort: Sort,
        limit: usize,
    ) -> SearchResult {
        let guard = self.loaded.read();
        let Some(loaded) = guard.as_ref() else {
            return SearchResult {
                creators: vec![],
                total: 0,
                size: 0,
                fetched_at: 0,
            };
        };
        let query = fold(query.trim());
        let mut hits: Vec<(u8, usize)> = loaded
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| service.is_none_or(|s| e.service == s))
            .filter_map(|(i, e)| rank(&loaded.folded[i], e, &query).map(|r| (r, i)))
            .collect();
        let entries = &loaded.entries;
        hits.sort_by(|(ra, a), (rb, b)| {
            let (ea, eb) = (&entries[*a], &entries[*b]);
            match sort {
                Sort::Relevance => ra.cmp(rb).then(eb.favorited.cmp(&ea.favorited)),
                Sort::Favorited => eb.favorited.cmp(&ea.favorited),
                Sort::Updated => eb.updated.cmp(&ea.updated),
                Sort::Name => loaded.folded[*a].cmp(&loaded.folded[*b]),
            }
        });
        let total = hits.len();
        let creators = hits
            .into_iter()
            .take(limit.clamp(1, MAX_RESULTS))
            .map(|(_, i)| {
                let e = &entries[i];
                Hit {
                    id: e.id.clone(),
                    name: if e.name.trim().is_empty() {
                        e.id.clone()
                    } else {
                        e.name.clone()
                    },
                    service: e.service.clone(),
                    public_id: e.public_id.clone(),
                    favorited: e.favorited,
                    updated: e.updated.saturating_mul(1000),
                    url: page::creator(&e.service, &e.id),
                    icon_url: page::icon(&e.service, &e.id),
                }
            })
            .collect();
        SearchResult {
            creators,
            total,
            size: entries.len(),
            fetched_at: loaded.fetched_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, name: &str, service: &str, favorited: i64) -> Entry {
        Entry {
            id: id.into(),
            name: name.into(),
            service: service.into(),
            public_id: Some(format!("p{id}")),
            favorited,
            updated: favorited,
        }
    }

    #[test]
    fn ranks_exact_then_prefix_then_contains_then_favorites() {
        let dir = Directory::new(std::env::temp_dir().join("pdl-dir-test"));
        dir.set(
            1,
            vec![
                entry("1", "Big Cat Art", "patreon", 900),
                entry("2", "cat", "fanbox", 5),
                entry("3", "Catherine", "patreon", 50),
                entry("4", "Dog", "patreon", 1000),
                entry("5", "ねこ 絵", "fanbox", 7),
            ],
        );
        let names = |r: SearchResult| r.creators.into_iter().map(|c| c.name).collect::<Vec<_>>();
        assert_eq!(
            names(dir.search("Cat", None, Sort::Relevance, 10)),
            ["cat", "Catherine", "Big Cat Art"]
        );
        assert_eq!(
            names(dir.search("bigcat", None, Sort::Relevance, 10)),
            ["Big Cat Art"]
        );
        assert_eq!(
            names(dir.search("ねこ絵", None, Sort::Relevance, 10)),
            ["ねこ 絵"]
        );
        assert_eq!(names(dir.search("4", None, Sort::Relevance, 10)), ["Dog"]);
        assert_eq!(
            names(dir.search("p3", None, Sort::Relevance, 10)),
            ["Catherine"]
        );
        let fanbox = dir.search("", Some("fanbox"), Sort::Favorited, 10);
        assert_eq!(fanbox.total, 2);
        assert_eq!(names(fanbox), ["ねこ 絵", "cat"]);
        let top = dir.search("", None, Sort::Relevance, 2);
        assert_eq!(top.total, 5);
        assert_eq!(top.size, 5);
        assert_eq!(names(top), ["Dog", "Big Cat Art"]);
    }

    #[test]
    fn reads_the_site_format() {
        let raw = r#"[{"id":"30500811","name":"BOKABA","public_id":"BOKABA","service":"patreon","indexed":1785423600,"updated":1794960000,"favorited":3729,"ever_imported":true},{"id":5,"name":null,"service":"fanbox"}]"#;
        let list: Vec<Entry> = serde_json::from_str(raw).unwrap();
        assert_eq!(list[1].name, "");
        assert_eq!(list[0].favorited, 3729);
        assert_eq!(list[1].id, "5");
        assert_eq!(list[1].favorited, 0);
    }
}
