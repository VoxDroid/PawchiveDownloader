//! The offline library: the images and videos in the download folders, to
//! look through without the site and without the network.
//!
//! It shows what the library recorded and also what is simply in the
//! download folders (saved by another install, before the history was
//! cleared, by the browser extension...), placed by the folder names the app
//! writes: `Site/Creator (service)/date Title [post id]/`.
//!
//! The window gets the files through the `pdl-media` protocol, which serves
//! only files the library recorded (by content hash) or that the last scan
//! of the download folders found (by an id made from path, size and time),
//! never by a path, and only images and videos: a page cannot use it to read
//! anything else on the computer.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::File;
use std::io::{BufWriter, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::pawchive::urls::{qualify, split_service, Site};
use crate::store::records::HistoryEntry;
use crate::store::KnownFile;

const IMAGES: &[&str] = &["png", "jpg", "jpeg", "jfif", "gif", "webp", "bmp", "avif"];
const VIDEOS: &[&str] = &["mp4", "m4v", "webm", "mov", "mkv", "ogv"];
/// Images the thumbnailer can read; others are shown as they are.
const THUMBNAILED: &[&str] = &["png", "jpg", "jpeg", "jfif", "gif", "webp", "bmp"];
/// Longest side of a thumbnail, in pixels (sharp at 2x in the grid).
pub const THUMB_SIZE: u32 = 480;
/// The most one answer returns of a video, so a long one starts at once and
/// seeks without being read whole.
const CHUNK: u64 = 4 * 1024 * 1024;
/// Thumbnails sit on the app's dark grey where an image is transparent.
const BACKDROP: [u8; 3] = [24, 27, 34];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaKind {
    Image,
    Video,
}

pub fn kind_of(path: &Path) -> Option<MediaKind> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    if IMAGES.contains(&ext.as_str()) {
        Some(MediaKind::Image)
    } else if VIDEOS.contains(&ext.as_str()) {
        Some(MediaKind::Video)
    } else {
        None
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaFile {
    pub hash: String,
    pub name: String,
    pub kind: MediaKind,
    pub size: u64,
    pub path: String,
    pub at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaPost {
    /// `service:creator:post`.
    pub key: String,
    pub service: String,
    pub site: String,
    pub creator_id: String,
    pub post_id: String,
    pub title: String,
    pub creator_name: String,
    /// `service:creator`, or a stand-in for creators known only by a folder.
    pub creator_key: String,
    /// The folder holding the post's files.
    pub folder: String,
    /// When its newest file was saved (epoch ms).
    pub at: i64,
    pub files: Vec<MediaFile>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaCreator {
    /// `service:creator`.
    pub key: String,
    pub service: String,
    pub site: String,
    pub creator_id: String,
    pub name: String,
    pub posts: u32,
    pub images: u32,
    pub videos: u32,
    pub bytes: u64,
    pub at: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Gallery {
    /// Most recently added first.
    pub creators: Vec<MediaCreator>,
    /// Most recently added first.
    pub posts: Vec<MediaPost>,
    pub images: u32,
    pub videos: u32,
    pub bytes: u64,
    /// Recorded images and videos that are no longer on disk.
    pub missing: u32,
    /// Files found in the folders: their ids and where they are, for serving.
    #[serde(skip)]
    pub found: Vec<(String, PathBuf)>,
}

/// An image or video found in a download folder.
#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    pub path: PathBuf,
    pub size: u64,
    /// Last change, epoch ms.
    pub modified: i64,
}

impl Found {
    /// Stands for the file in addresses: from its path, size and time, so
    /// a changed file gets a new one (and a new thumbnail).
    pub fn id(&self) -> String {
        use sha2::{Digest, Sha256};
        let key = format!(
            "{}|{}|{}",
            self.path.to_string_lossy(),
            self.size,
            self.modified
        );
        hex::encode(Sha256::digest(key.as_bytes()))
    }
}

/// How deep and how much a scan looks, so a download folder set to a whole
/// drive cannot make it run for ever.
const SCAN_DEPTH: usize = 12;
const SCAN_ENTRIES: usize = 500_000;

/// The images and videos under these folders. Hidden folders (the staging
/// area) and links are skipped. Blocking.
pub fn scan(roots: &[PathBuf]) -> Vec<Found> {
    let mut out = Vec::new();
    let mut visited = HashSet::new();
    let mut budget = SCAN_ENTRIES;
    for root in roots {
        let mut stack = vec![(root.clone(), 0usize)];
        while let Some((dir, depth)) = stack.pop() {
            if !visited.insert(fold_path(&dir)) {
                continue;
            }
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                if budget == 0 {
                    return out;
                }
                budget -= 1;
                if entry.file_name().to_string_lossy().starts_with('.') {
                    continue;
                }
                let Ok(kind) = entry.file_type() else {
                    continue;
                };
                let path = entry.path();
                if kind.is_dir() {
                    if depth < SCAN_DEPTH {
                        stack.push((path, depth + 1));
                    }
                } else if kind.is_file() && kind_of(&path).is_some() {
                    let Ok(meta) = entry.metadata() else {
                        continue;
                    };
                    let modified = meta
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map_or(0, |d| d.as_millis() as i64);
                    out.push(Found {
                        path,
                        size: meta.len(),
                        modified,
                    });
                }
            }
        }
    }
    out
}

/// A path as a comparable string: one kind of slash, and any case where the
/// file system ignores case (Windows, macOS). On Linux `Art` and `art` are
/// two folders, and both are scanned.
pub fn fold_path(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let text = text.trim_end_matches('/');
    if cfg!(any(windows, target_os = "macos")) {
        text.to_lowercase()
    } else {
        text.to_string()
    }
}

/// The folders to scan: these, without those inside another, that exist.
pub fn roots(candidates: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut list: Vec<(String, PathBuf)> = candidates
        .into_iter()
        .filter(|p| p.is_absolute() && p.is_dir())
        .map(|p| (fold_path(&p), p))
        .collect();
    list.sort_by(|a, b| a.0.len().cmp(&b.0.len()).then_with(|| a.0.cmp(&b.0)));
    let mut kept: Vec<(String, PathBuf)> = Vec::new();
    for (key, path) in list {
        let inside = kept
            .iter()
            .any(|(k, _)| key == *k || key.starts_with(&format!("{k}/")));
        if !inside {
            kept.push((key, path));
        }
    }
    kept.into_iter().map(|(_, p)| p).collect()
}

/// "... [175803858]" -> "175803858".
fn post_id_of(name: &str) -> Option<&str> {
    let inner = name.trim().strip_suffix(']')?;
    let id = &inner[inner.rfind('[')? + 1..];
    (!id.is_empty()
        && id.len() <= 128
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')))
    .then_some(id)
}

/// "Aster Vale (patreon)" -> ("Aster Vale", "patreon").
fn creator_and_service(name: &str) -> Option<(String, String)> {
    let inner = name.trim().strip_suffix(')')?;
    let open = inner.rfind(" (")?;
    let service = &inner[open + 2..];
    let ok = (2..=32).contains(&service.len())
        && service
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'));
    let creator = inner[..open].trim();
    (ok && !creator.is_empty()).then(|| (creator.to_string(), service.to_ascii_lowercase()))
}

/// Where a found file belongs, read from its folders.
#[derive(Debug, PartialEq)]
struct Place {
    /// The qualified service (`fanbox`, `fanbox@kemono`), when known.
    service: Option<String>,
    site: Option<Site>,
    creator: Option<String>,
    post_id: Option<String>,
    title: String,
    folder: PathBuf,
    creator_folder: Option<PathBuf>,
}

fn name_of(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn place_of(file: &Path) -> Place {
    // The post folder is the nearest one named like a post (files of
    // unpacked archives and outside links sit a level or two below it).
    let mut post_folder = None;
    let mut dir = file.parent();
    for _ in 0..4 {
        let Some(d) = dir else { break };
        if post_id_of(&name_of(d)).is_some() {
            post_folder = Some(d);
            break;
        }
        dir = d.parent();
    }
    match post_folder {
        Some(folder) => {
            let name = name_of(folder);
            let post_id = post_id_of(&name).map(str::to_string);
            let creator_dir = folder.parent();
            let creator = creator_dir.and_then(|d| creator_and_service(&name_of(d)));
            let site = creator_dir
                .and_then(Path::parent)
                .and_then(|d| Site::parse(&name_of(d)));
            let service = creator
                .as_ref()
                .map(|(_, s)| qualify(site.unwrap_or_default(), s));
            let title = title_from_folder(&name, post_id.as_deref().unwrap_or(""));
            Place {
                service,
                site: creator.as_ref().map(|_| site.unwrap_or_default()),
                creator: creator.map(|(c, _)| c),
                post_id,
                title: if title.is_empty() { name } else { title },
                folder: folder.to_path_buf(),
                creator_folder: creator_dir.map(Path::to_path_buf),
            }
        }
        None => {
            // Not laid out by the app: one group per folder.
            let folder = file.parent().unwrap_or(file).to_path_buf();
            let parent = folder.parent().map(Path::to_path_buf);
            Place {
                service: None,
                site: None,
                creator: parent.as_deref().map(name_of).filter(|n| !n.is_empty()),
                post_id: None,
                title: name_of(&folder),
                folder,
                creator_folder: parent,
            }
        }
    }
}

/// "2026-11-23 Bull (Bull) [175803858]" -> "Bull (Bull)", the default
/// folder name of a post; anything else is kept as it is.
fn title_from_folder(folder: &str, post_id: &str) -> String {
    let mut name = folder.trim();
    if let Some(rest) = name.strip_suffix(&format!("[{post_id}]")) {
        name = rest.trim_end();
    }
    let bytes = name.as_bytes();
    let dated = bytes.len() > 11
        && bytes[..10].iter().enumerate().all(|(i, b)| {
            if i == 4 || i == 7 {
                *b == b'-'
            } else {
                b.is_ascii_digit()
            }
        })
        && bytes[10] == b' ';
    if dated {
        name = &name[11..];
    }
    name.trim().to_string()
}

/// "Aster Vale (patreon)" -> "Aster Vale", only when it ends with the service.
fn creator_from_folder(folder: &str, service: &str) -> Option<String> {
    let name = folder
        .trim()
        .strip_suffix(&format!("({service})"))?
        .trim_end();
    (!name.is_empty()).then(|| name.to_string())
}

/// Groups the recorded files into posts and creators, with the names the
/// history knows (or the folder names when it does not). `size_of` answers
/// None for a file that is no longer there.
/// Groups the recorded files and the ones found in the folders into posts
/// and creators, with the names the history and the watchlist know (or the
/// folder names when they do not). `size_of` answers None for a recorded
/// file that is no longer there. `known` lists creators as (qualified
/// service, id, name), to match folders to creators.
pub fn build(
    files: &[(KnownFile, i64)],
    history: &[HistoryEntry],
    known: &[(String, String, String)],
    found: &[Found],
    size_of: impl Fn(&Path) -> Option<u64>,
) -> Gallery {
    let mut creator_names: HashMap<String, String> = HashMap::new();
    let mut post_titles: HashMap<String, String> = HashMap::new();
    for h in history {
        if let Some(c) = &h.creator {
            creator_names
                .entry(format!("{}:{}", c.service, c.id))
                .or_insert_with(|| c.name.clone());
        }
        // A download of one post is named after the post.
        let named_after_creator = h.creator.as_ref().is_some_and(|c| c.name == h.title);
        if let [only] = h.refs.as_slice() {
            if !named_after_creator && !h.title.trim().is_empty() {
                post_titles
                    .entry(only.key())
                    .or_insert_with(|| h.title.clone());
            }
        }
    }

    let mut gallery = Gallery::default();
    let mut posts: BTreeMap<String, MediaPost> = BTreeMap::new();
    for (f, at) in files {
        let path = Path::new(&f.path);
        let Some(kind) = kind_of(path) else {
            continue;
        };
        let mut parts = f.post.splitn(3, ':');
        let (Some(service), Some(creator_id), Some(post_id)) =
            (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let Some(size) = size_of(path) else {
            gallery.missing += 1;
            continue;
        };
        let parent = path.parent();
        let post = posts.entry(f.post.clone()).or_insert_with(|| {
            let (site, bare) = split_service(service);
            let folder_name = |p: Option<&Path>| {
                p.and_then(|p| p.file_name())
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default()
            };
            let title = post_titles
                .get(&f.post)
                .cloned()
                .unwrap_or_else(|| title_from_folder(&folder_name(parent), post_id));
            let creator_name = creator_names
                .get(&format!("{service}:{creator_id}"))
                .cloned()
                .or_else(|| creator_from_folder(&folder_name(parent.and_then(Path::parent)), bare))
                .unwrap_or_else(|| creator_id.to_string());
            MediaPost {
                key: f.post.clone(),
                creator_key: format!("{service}:{creator_id}"),
                service: service.to_string(),
                site: site.id().to_string(),
                creator_id: creator_id.to_string(),
                post_id: post_id.to_string(),
                title: if title.is_empty() {
                    post_id.to_string()
                } else {
                    title
                },
                creator_name,
                folder: parent
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                at: 0,
                files: vec![],
            }
        });
        post.at = post.at.max(*at);
        post.files.push(MediaFile {
            hash: f.hash.clone(),
            name: path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            kind,
            size,
            path: f.path.clone(),
            at: *at,
        });
        match kind {
            MediaKind::Image => gallery.images += 1,
            MediaKind::Video => gallery.videos += 1,
        }
        gallery.bytes += size;
    }

    // Files in the folders the library has no record of: placed by their
    // folder names and joined to the recorded post or creator they belong to.
    let recorded: HashSet<String> = files
        .iter()
        .map(|(f, _)| fold_path(Path::new(&f.path)))
        .collect();
    let fold = |s: &str| s.trim().to_lowercase();
    let mut creator_ids: HashMap<(String, String), String> = HashMap::new();
    for (service, id, name) in known {
        creator_ids
            .entry((service.clone(), fold(name)))
            .or_insert_with(|| id.clone());
    }
    for h in history {
        if let Some(c) = &h.creator {
            creator_ids
                .entry((c.service.clone(), fold(&c.name)))
                .or_insert_with(|| c.id.clone());
        }
    }
    for p in posts.values() {
        creator_ids
            .entry((p.service.clone(), fold(&p.creator_name)))
            .or_insert_with(|| p.creator_id.clone());
    }
    let recorded_posts: HashMap<(String, String), String> = posts
        .values()
        .map(|p| ((p.service.clone(), p.post_id.clone()), p.key.clone()))
        .collect();
    for f in found {
        if recorded.contains(&fold_path(&f.path)) {
            continue;
        }
        let Some(kind) = kind_of(&f.path) else {
            continue;
        };
        let place = place_of(&f.path);
        let creator_name = place.creator.clone().unwrap_or_default();
        let creator_id = match &place.service {
            Some(service) => creator_ids
                .get(&(service.clone(), fold(&creator_name)))
                .cloned()
                .unwrap_or_default(),
            None => String::new(),
        };
        let folder_key = |p: &Path| format!("folder:{}", fold_path(p));
        let key = match (&place.service, &place.post_id) {
            (Some(service), Some(post_id)) => recorded_posts
                .get(&(service.clone(), post_id.clone()))
                .cloned()
                .unwrap_or_else(|| {
                    if creator_id.is_empty() {
                        format!("folder:{service}:{}:{post_id}", fold(&creator_name))
                    } else {
                        format!("{service}:{creator_id}:{post_id}")
                    }
                }),
            _ => folder_key(&place.folder),
        };
        let post = posts.entry(key.clone()).or_insert_with(|| {
            let service = place.service.clone().unwrap_or_default();
            let creator_key = if !creator_id.is_empty() {
                format!("{service}:{creator_id}")
            } else if !service.is_empty() {
                format!("folder:{service}:{}", fold(&creator_name))
            } else {
                place
                    .creator_folder
                    .as_deref()
                    .map(folder_key)
                    .unwrap_or_else(|| folder_key(&place.folder))
            };
            MediaPost {
                key: key.clone(),
                creator_key,
                site: place.site.map(|s| s.id().to_string()).unwrap_or_default(),
                service,
                creator_id: creator_id.clone(),
                post_id: place.post_id.clone().unwrap_or_default(),
                title: place.title.clone(),
                creator_name: if creator_name.is_empty() {
                    place.title.clone()
                } else {
                    creator_name.clone()
                },
                folder: place.folder.to_string_lossy().into_owned(),
                at: 0,
                files: vec![],
            }
        });
        post.at = post.at.max(f.modified);
        let id = f.id();
        post.files.push(MediaFile {
            hash: id.clone(),
            name: name_of(&f.path),
            kind,
            size: f.size,
            path: f.path.to_string_lossy().into_owned(),
            at: f.modified,
        });
        gallery.found.push((id, f.path.clone()));
        match kind {
            MediaKind::Image => gallery.images += 1,
            MediaKind::Video => gallery.videos += 1,
        }
        gallery.bytes += f.size;
    }

    let mut creators: BTreeMap<String, MediaCreator> = BTreeMap::new();
    let mut posts: Vec<MediaPost> = posts.into_values().collect();
    for post in &mut posts {
        post.files.sort_by(|a, b| natural(&a.name, &b.name));
        let c = creators
            .entry(post.creator_key.clone())
            .or_insert_with(|| MediaCreator {
                key: post.creator_key.clone(),
                service: post.service.clone(),
                site: post.site.clone(),
                creator_id: post.creator_id.clone(),
                name: post.creator_name.clone(),
                posts: 0,
                images: 0,
                videos: 0,
                bytes: 0,
                at: 0,
            });
        c.posts += 1;
        c.at = c.at.max(post.at);
        for f in &post.files {
            c.bytes += f.size;
            match f.kind {
                MediaKind::Image => c.images += 1,
                MediaKind::Video => c.videos += 1,
            }
        }
    }
    posts.sort_by(|a, b| b.at.cmp(&a.at).then_with(|| a.key.cmp(&b.key)));
    let mut creators: Vec<MediaCreator> = creators.into_values().collect();
    creators.sort_by(|a, b| b.at.cmp(&a.at).then_with(|| a.name.cmp(&b.name)));
    gallery.posts = posts;
    gallery.creators = creators;
    gallery
}

/// "2.png" before "10.png", as people number files.
fn natural(a: &str, b: &str) -> std::cmp::Ordering {
    fn key(s: &str) -> Vec<(String, u64)> {
        let mut out = vec![];
        let mut text = String::new();
        let mut digits = String::new();
        for ch in s.to_lowercase().chars() {
            if ch.is_ascii_digit() {
                digits.push(ch);
            } else {
                if !digits.is_empty() {
                    out.push((
                        std::mem::take(&mut text),
                        digits.parse().unwrap_or(u64::MAX),
                    ));
                    digits.clear();
                }
                text.push(ch);
            }
        }
        out.push((text, digits.parse().unwrap_or(0)));
        out
    }
    key(a).cmp(&key(b))
}

// ------------------------------------------------------------------ serving

/// An answer of the `pdl-media` protocol.
#[derive(Debug)]
pub struct Served {
    pub status: u16,
    pub headers: Vec<(&'static str, String)>,
    pub body: Vec<u8>,
}

impl Served {
    fn status(status: u16) -> Self {
        Served {
            status,
            headers: vec![],
            body: vec![],
        }
    }
}

pub fn is_hash(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|b| b.is_ascii_hexdigit())
}

fn mime(path: &Path) -> &'static str {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" | "jfif" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "avif" => "image/avif",
        "mp4" | "m4v" => "video/mp4",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        "mkv" => "video/x-matroska",
        "ogv" => "video/ogg",
        _ => "application/octet-stream",
    }
}

/// `bytes=START-END` (END optional) against a file of `len` bytes, clamped.
/// At most CHUNK bytes are answered at a time, whatever was asked: a player
/// asking for `bytes=0-` and its full length must not read a 4 GB video into
/// memory. It asks again for the rest, as with any partial answer.
fn parse_range(header: &str, len: u64) -> Option<(u64, u64)> {
    let spec = header.trim().strip_prefix("bytes=")?;
    let (start, end) = spec.split(',').next()?.split_once('-')?;
    if start.trim().is_empty() {
        // The last N bytes.
        let n: u64 = end.trim().parse().ok()?;
        return (n > 0 && len > 0).then(|| (len.saturating_sub(n.min(CHUNK)), len - 1));
    }
    let start: u64 = start.trim().parse().ok()?;
    if start >= len {
        return None;
    }
    let most = start.saturating_add(CHUNK).min(len) - 1;
    let end = match end.trim() {
        "" => most,
        e => e.parse::<u64>().ok()?.min(most),
    };
    (end >= start).then_some((start, end))
}

fn read_part(path: &Path, start: u64, end: u64) -> std::io::Result<Vec<u8>> {
    let mut file = File::open(path)?;
    file.seek(SeekFrom::Start(start))?;
    let mut buf = vec![0; (end - start + 1) as usize];
    file.read_exact(&mut buf)?;
    Ok(buf)
}

/// Serves a recorded file: the whole image, or a piece of a video.
pub fn serve_file(path: &Path, range: Option<&str>) -> Served {
    let Some(kind) = kind_of(path) else {
        return Served::status(403);
    };
    let Ok(len) = std::fs::metadata(path).map(|m| m.len()) else {
        return Served::status(404);
    };
    let mut headers = vec![
        ("Content-Type", mime(path).to_string()),
        ("Accept-Ranges", "bytes".to_string()),
        // The address is the file's content hash, so it never changes.
        ("Cache-Control", "max-age=31536000, immutable".to_string()),
    ];
    let wanted = match range {
        Some(r) => match parse_range(r, len) {
            Some(span) => Some(span),
            None => {
                headers.push(("Content-Range", format!("bytes */{len}")));
                return Served {
                    status: 416,
                    headers,
                    body: vec![],
                };
            }
        },
        // A big video without a range: answer with its start, the player
        // asks for the rest.
        None if kind == MediaKind::Video && len > CHUNK * 4 => Some((0, CHUNK - 1)),
        None => None,
    };
    match wanted {
        Some((start, end)) => match read_part(path, start, end) {
            Ok(body) => {
                headers.push(("Content-Range", format!("bytes {start}-{end}/{len}")));
                Served {
                    status: 206,
                    headers,
                    body,
                }
            }
            Err(_) => Served::status(500),
        },
        None => match std::fs::read(path) {
            Ok(body) => Served {
                status: 200,
                headers,
                body,
            },
            Err(_) => Served::status(500),
        },
    }
}

/// Where the thumbnail of a file is kept.
pub fn thumb_path(dir: &Path, hash: &str) -> PathBuf {
    dir.join(format!("{}.jpg", hash.to_ascii_lowercase()))
}

/// Whether a thumbnail can be made of this file.
pub fn can_thumbnail(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| THUMBNAILED.contains(&e.to_ascii_lowercase().as_str()))
}

/// Makes a JPEG thumbnail (THUMB_SIZE on its longest side). Blocking.
pub fn make_thumbnail(src: &Path, dest: &Path) -> Result<(), String> {
    let image = image::ImageReader::open(src)
        .map_err(|e| e.to_string())?
        .with_guessed_format()
        .map_err(|e| e.to_string())?
        .decode()
        .map_err(|e| e.to_string())?;
    let small = image.thumbnail(THUMB_SIZE, THUMB_SIZE).to_rgba8();
    let mut rgb = image::RgbImage::new(small.width(), small.height());
    for (x, y, p) in small.enumerate_pixels() {
        let a = u16::from(p[3]);
        let mix = |c: u8, b: u8| ((u16::from(c) * a + u16::from(b) * (255 - a)) / 255) as u8;
        rgb.put_pixel(
            x,
            y,
            image::Rgb([
                mix(p[0], BACKDROP[0]),
                mix(p[1], BACKDROP[1]),
                mix(p[2], BACKDROP[2]),
            ]),
        );
    }
    let dir = dest.parent().ok_or("no folder")?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    // Written aside and moved in, so a half-written thumbnail is never served.
    let tmp = dir.join(format!(
        ".{}.{:08x}.part",
        dest.file_name().and_then(|n| n.to_str()).unwrap_or("thumb"),
        rand::random::<u32>()
    ));
    {
        let mut out = BufWriter::new(File::create(&tmp).map_err(|e| e.to_string())?);
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 82)
            .encode_image(&rgb)
            .map_err(|e| e.to_string())?;
    }
    std::fs::rename(&tmp, dest).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        e.to_string()
    })
}

/// Serves a thumbnail made earlier (cached for good: it never changes).
pub fn serve_thumb(path: &Path) -> Served {
    match std::fs::read(path) {
        Ok(body) => Served {
            status: 200,
            headers: vec![
                ("Content-Type", "image/jpeg".to_string()),
                ("Cache-Control", "max-age=31536000, immutable".to_string()),
            ],
            body,
        },
        Err(_) => Served::status(404),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::model::{CreatorInfo, PostRef};

    fn known(hash: &str, post: &str, path: &Path) -> KnownFile {
        KnownFile {
            hash: hash.into(),
            post: post.into(),
            path: path.to_string_lossy().into_owned(),
        }
    }

    #[test]
    fn names_posts_and_creators_from_history_or_folders() {
        let dir = tempfile::tempdir().unwrap();
        let aster = dir.path().join("Pawchive/Aster Vale (patreon)");
        let post_a = aster.join("2026-11-01 Forest [1]");
        let post_b = dir.path().join("Kemono/Kemo (fanbox)/2026-10-02 Set [9]");
        for (folder, name) in [
            (&post_a, "10_b.png"),
            (&post_a, "2_a.png"),
            (&post_a, "clip.MP4"),
            (&post_a, "notes.txt"),
            (&post_b, "x.jpg"),
        ] {
            std::fs::create_dir_all(folder).unwrap();
            std::fs::write(folder.join(name), b"1234").unwrap();
        }
        let files = vec![
            (known("h1", "patreon:7:1", &post_a.join("10_b.png")), 5),
            (known("h2", "patreon:7:1", &post_a.join("2_a.png")), 6),
            (known("h3", "patreon:7:1", &post_a.join("clip.MP4")), 7),
            (known("h4", "patreon:7:1", &post_a.join("notes.txt")), 7),
            (known("h5", "fanbox@kemono:9:9", &post_b.join("x.jpg")), 9),
            (
                known("h6", "fanbox@kemono:9:9", &post_b.join("gone.png")),
                9,
            ),
        ];
        let mut h = HistoryEntry::sample("j", 1);
        h.title = "Forest Studies".into();
        h.creator = Some(CreatorInfo {
            service: "patreon".into(),
            id: "7".into(),
            name: "Aster".into(),
        });
        h.refs = vec![PostRef {
            service: "patreon".into(),
            creator_id: "7".into(),
            post_id: "1".into(),
        }];
        let g = build(&files, &[h], &[], &[], |p| {
            std::fs::metadata(p).ok().map(|m| m.len())
        });
        assert_eq!((g.images, g.videos, g.bytes, g.missing), (3, 1, 16, 1));
        // Newest first; names from the history, else from the folders.
        assert_eq!(g.posts[0].title, "Set");
        assert_eq!(g.posts[0].creator_name, "Kemo");
        assert_eq!(g.posts[0].site, "kemono");
        assert_eq!(g.posts[1].title, "Forest Studies");
        assert_eq!(g.posts[1].creator_name, "Aster");
        let names: Vec<&str> = g.posts[1].files.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["2_a.png", "10_b.png", "clip.MP4"]);
        assert_eq!(g.posts[1].files[2].kind, MediaKind::Video);
        assert_eq!(g.creators.len(), 2);
        assert_eq!(g.creators[1].images, 2);
        assert_eq!(g.creators[1].videos, 1);
    }

    #[test]
    fn places_found_files_by_their_folders() {
        let base = Path::new("/dl");
        let p = place_of(&base.join("Kemono/Kemo (fanbox)/2026-10-02 Set [9]/x.jpg"));
        assert_eq!(p.service.as_deref(), Some("fanbox@kemono"));
        assert_eq!(p.site, Some(Site::Kemono));
        assert_eq!(p.creator.as_deref(), Some("Kemo"));
        assert_eq!(p.post_id.as_deref(), Some("9"));
        assert_eq!(p.title, "Set");
        // Unpacked archives and outside links sit below the post folder.
        let p =
            place_of(&base.join("Pawchive/Aster (patreon)/2026-11-01 Forest [1]/Links/pack/a.png"));
        assert_eq!(
            (p.post_id.as_deref(), p.service.as_deref()),
            (Some("1"), Some("patreon"))
        );
        // The old layout had no site folder: Pawchive.
        let p = place_of(&base.join("Aster (patreon)/2026-11-01 Forest [1]/a.png"));
        assert_eq!(
            (p.site, p.service.as_deref()),
            (Some(Site::Pawchive), Some("patreon"))
        );
        // Anything else is one group per folder.
        let p = place_of(&base.join("misc/wallpapers/a.png"));
        assert_eq!(
            (p.post_id, p.service, p.title.as_str()),
            (None, None, "wallpapers")
        );
        assert_eq!(p.creator.as_deref(), Some("misc"));
    }

    #[test]
    fn scans_folders_once_and_skips_hidden_ones() {
        let dir = tempfile::tempdir().unwrap();
        let post = dir.path().join("Pawchive/A (patreon)/2026-01-01 T [5]");
        std::fs::create_dir_all(&post).unwrap();
        std::fs::create_dir_all(dir.path().join(".pawchive-staging/job")).unwrap();
        for f in ["a.png", "b.MP4", "c.txt"] {
            std::fs::write(post.join(f), b"x").unwrap();
        }
        std::fs::write(dir.path().join(".pawchive-staging/job/d.png"), b"x").unwrap();
        // The root and a folder inside it: scanned once.
        let roots = roots(vec![
            dir.path().to_path_buf(),
            post.clone(),
            PathBuf::from("/nope"),
        ]);
        assert_eq!(roots, vec![dir.path().to_path_buf()]);
        let found = scan(&roots);
        let mut names: Vec<String> = found.iter().map(|f| name_of(&f.path)).collect();
        names.sort();
        assert_eq!(names, ["a.png", "b.MP4"]);
        assert!(is_hash(&found[0].id()));
        assert_ne!(found[0].id(), found[1].id());
    }

    #[test]
    fn reads_folder_names() {
        assert_eq!(
            title_from_folder("2026-11-23 Bull (Bull) [175803858]", "175803858"),
            "Bull (Bull)"
        );
        assert_eq!(title_from_folder("Just a name", "5"), "Just a name");
        assert_eq!(
            creator_from_folder("Aster Vale (patreon)", "patreon").as_deref(),
            Some("Aster Vale")
        );
        assert_eq!(creator_from_folder("Aster Vale", "patreon"), None);
    }

    #[test]
    fn serves_ranges_of_videos_and_whole_images() {
        let dir = tempfile::tempdir().unwrap();
        let video = dir.path().join("v.mp4");
        std::fs::write(&video, (0..100u8).collect::<Vec<_>>()).unwrap();
        let part = serve_file(&video, Some("bytes=10-19"));
        assert_eq!(part.status, 206);
        assert_eq!(part.body, (10..20u8).collect::<Vec<_>>());
        assert!(part
            .headers
            .contains(&("Content-Range", "bytes 10-19/100".to_string())));
        let open = serve_file(&video, Some("bytes=90-"));
        assert_eq!(open.body.len(), 10);
        let tail = serve_file(&video, Some("bytes=-5"));
        assert_eq!(tail.body, (95..100u8).collect::<Vec<_>>());
        assert_eq!(serve_file(&video, Some("bytes=500-")).status, 416);
        assert_eq!(serve_file(&video, None).status, 200);
        // A long range is answered a chunk at a time, never the whole file.
        let len = CHUNK * 3;
        assert_eq!(
            parse_range(&format!("bytes=0-{}", len - 1), len),
            Some((0, CHUNK - 1))
        );
        assert_eq!(
            parse_range(&format!("bytes=-{len}"), len),
            Some((len - CHUNK, len - 1))
        );
        assert_eq!(parse_range("bytes=5-2", len), None);

        let image = dir.path().join("i.png");
        std::fs::write(&image, b"png").unwrap();
        let whole = serve_file(&image, None);
        assert_eq!(
            (whole.status, whole.body.as_slice()),
            (200, b"png".as_slice())
        );
        assert!(whole
            .headers
            .contains(&("Content-Type", "image/png".to_string())));
        // Never anything that is not an image or a video.
        let text = dir.path().join("secret.txt");
        std::fs::write(&text, b"x").unwrap();
        assert_eq!(serve_file(&text, None).status, 403);
        assert!(is_hash(&"a".repeat(64)));
        assert!(!is_hash("../etc/passwd"));
    }

    #[test]
    fn makes_thumbnails_over_the_backdrop() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("big.png");
        let mut img = image::RgbaImage::new(1200, 600);
        for p in img.pixels_mut() {
            *p = image::Rgba([255, 0, 0, 0]);
        }
        img.save(&src).unwrap();
        let dest = thumb_path(&dir.path().join("thumbs"), &"AB".repeat(32));
        make_thumbnail(&src, &dest).unwrap();
        let thumb = image::open(&dest).unwrap().to_rgb8();
        assert_eq!(
            (thumb.width(), thumb.height()),
            (THUMB_SIZE, THUMB_SIZE / 2)
        );
        // Fully transparent red becomes the backdrop, not black or red.
        let px = thumb.get_pixel(10, 10);
        assert!(px[0] < 40 && px[2] > 20, "{px:?}");
        assert!(can_thumbnail(&src));
        assert!(!can_thumbnail(Path::new("a.mp4")));
        assert_eq!(serve_thumb(&dest).status, 200);
    }
}

#[cfg(test)]
mod timing {
    use super::*;

    /// Not a test: `cargo test --lib gallery::timing -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn thumbnail_speed() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("big.png");
        let mut img = image::RgbaImage::new(3000, 3000);
        for (x, y, p) in img.enumerate_pixels_mut() {
            *p = image::Rgba([(x % 251) as u8, (y % 241) as u8, ((x ^ y) % 239) as u8, 255]);
        }
        img.save(&src).unwrap();
        let jpg = dir.path().join("big.jpg");
        image::DynamicImage::ImageRgba8(img)
            .to_rgb8()
            .save(&jpg)
            .unwrap();
        for (name, path) in [("png", &src), ("jpg", &jpg)] {
            let t = std::time::Instant::now();
            make_thumbnail(path, &dir.path().join(format!("t-{name}.jpg"))).unwrap();
            let size = std::fs::metadata(path).unwrap().len() / 1024;
            println!("{name} {size} KiB: thumbnail in {:?}", t.elapsed());
            let t = std::time::Instant::now();
            let served = serve_file(path, None);
            println!(
                "{name}: served whole in {:?} ({} KiB)",
                t.elapsed(),
                served.body.len() / 1024
            );
        }
    }
}
