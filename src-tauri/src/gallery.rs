//! The offline library: the images and videos the app downloaded, to look
//! through without the site and without the network.
//!
//! The window gets the files through the `pdl-media` protocol, which serves
//! only files the library recorded, picked by content hash and never by
//! path, and only images and videos: a page cannot use it to read anything
//! else on the computer.

use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::io::{BufWriter, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::pawchive::urls::split_service;
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
pub fn build(
    files: &[(KnownFile, i64)],
    history: &[HistoryEntry],
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

    let mut creators: BTreeMap<String, MediaCreator> = BTreeMap::new();
    let mut posts: Vec<MediaPost> = posts.into_values().collect();
    for post in &mut posts {
        post.files.sort_by(|a, b| natural(&a.name, &b.name));
        let c = creators
            .entry(format!("{}:{}", post.service, post.creator_id))
            .or_insert_with(|| MediaCreator {
                key: format!("{}:{}", post.service, post.creator_id),
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
fn parse_range(header: &str, len: u64) -> Option<(u64, u64)> {
    let spec = header.trim().strip_prefix("bytes=")?;
    let (start, end) = spec.split(',').next()?.split_once('-')?;
    if start.trim().is_empty() {
        // The last N bytes.
        let n: u64 = end.trim().parse().ok()?;
        return (n > 0 && len > 0).then(|| (len.saturating_sub(n), len - 1));
    }
    let start: u64 = start.trim().parse().ok()?;
    if start >= len {
        return None;
    }
    let end = match end.trim() {
        "" => (start + CHUNK).min(len) - 1,
        e => e.parse::<u64>().ok()?.min(len - 1),
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
        let g = build(&files, &[h], |p| std::fs::metadata(p).ok().map(|m| m.len()));
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
