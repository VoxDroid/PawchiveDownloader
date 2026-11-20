//! Writes ZIP archives from files fetched into a staging folder. Archives use
//! ZIP64 where needed, so there is no 4 GiB limit; they can optionally be
//! split into parts of a maximum size. Media is stored as is (it is already
//! compressed), text is deflated.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::pawchive::types::FileCategory;

pub enum EntryData {
    File(PathBuf),
    Text(String),
}

pub struct ArchiveEntry {
    pub name: String,
    pub data: EntryData,
    pub category: FileCategory,
}

impl ArchiveEntry {
    fn size(&self) -> u64 {
        match &self.data {
            EntryData::File(p) => std::fs::metadata(p).map(|m| m.len()).unwrap_or(0),
            EntryData::Text(t) => t.len() as u64,
        }
    }
}

/// `Name.zip` for the first part, `Name (part 2).zip` and so on after it.
pub fn part_name(target: &Path, part: u32) -> PathBuf {
    if part <= 1 {
        return target.to_path_buf();
    }
    let stem = target
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    target.with_file_name(format!("{stem} (part {part}).zip"))
}

/// Unique names inside one archive (`a.png`, `a (1).png`).
fn unique_name(name: &str, used: &mut std::collections::HashSet<String>) -> String {
    let mut candidate = name.to_string();
    let mut n = 1;
    while !used.insert(candidate.to_lowercase()) {
        let path = Path::new(name);
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        candidate = match path.extension() {
            Some(ext) => format!("{stem} ({n}).{}", ext.to_string_lossy()),
            None => format!("{stem} ({n})"),
        };
        n += 1;
    }
    candidate
}

/// Builds the archive(s). Each part is written as `<name>.part` and renamed
/// when complete. `max_part_bytes` of 0 means a single archive. Returns the
/// files written and their total size.
pub fn build(
    target: &Path,
    entries: &[ArchiveEntry],
    max_part_bytes: u64,
    mut on_bytes: impl FnMut(u64),
) -> std::io::Result<(Vec<PathBuf>, u64)> {
    if let Some(dir) = target.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut written = Vec::new();
    let mut total = 0u64;
    let mut part = 1u32;
    let mut used = std::collections::HashSet::new();
    let mut current_size = 0u64;
    let open = |part: u32| -> std::io::Result<(PathBuf, ZipWriter<BufWriter<File>>)> {
        let path = part_name(target, part);
        let tmp = crate::jobs::transfer::part_path(&path);
        let file = File::create(&tmp)?;
        Ok((
            path,
            ZipWriter::new(BufWriter::with_capacity(1 << 20, file)),
        ))
    };
    let (mut path, mut zip) = open(part)?;
    let mut entries_in_part = 0;
    for entry in entries {
        let size = entry.size();
        if max_part_bytes > 0 && entries_in_part > 0 && current_size + size > max_part_bytes {
            close(zip, &path)?;
            total += current_size;
            written.push(path);
            part += 1;
            (path, zip) = open(part)?;
            current_size = 0;
            entries_in_part = 0;
            used.clear();
        }
        let method = match entry.category {
            FileCategory::Text | FileCategory::Document | FileCategory::Other => {
                CompressionMethod::Deflated
            }
            _ => CompressionMethod::Stored,
        };
        let options = SimpleFileOptions::default()
            .compression_method(method)
            .large_file(size >= 0xFFFF_0000)
            .unix_permissions(0o644);
        zip.start_file(unique_name(&entry.name, &mut used), options)
            .map_err(std::io::Error::other)?;
        match &entry.data {
            EntryData::Text(text) => zip.write_all(text.as_bytes())?,
            EntryData::File(p) => {
                let mut f = File::open(p)?;
                std::io::copy(&mut f, &mut zip)?;
            }
        }
        current_size += size;
        entries_in_part += 1;
        on_bytes(size);
    }
    close(zip, &path)?;
    total += current_size;
    written.push(path);
    Ok((written, total))
}

fn close(zip: ZipWriter<BufWriter<File>>, path: &Path) -> std::io::Result<()> {
    let mut inner = zip.finish().map_err(std::io::Error::other)?;
    inner.flush()?;
    inner.get_ref().sync_all().ok();
    drop(inner);
    std::fs::rename(crate::jobs::transfer::part_path(path), path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    fn names(path: &Path) -> Vec<String> {
        let mut archive = zip::ZipArchive::new(File::open(path).unwrap()).unwrap();
        (0..archive.len())
            .map(|i| archive.by_index(i).unwrap().name().unwrap().to_string())
            .collect()
    }

    #[test]
    fn builds_single_and_split_archives() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.bin");
        std::fs::write(&a, vec![7u8; 1000]).unwrap();
        let entries = vec![
            ArchiveEntry {
                name: "img.png".into(),
                data: EntryData::File(a.clone()),
                category: FileCategory::Image,
            },
            ArchiveEntry {
                name: "img.png".into(),
                data: EntryData::File(a.clone()),
                category: FileCategory::Image,
            },
            ArchiveEntry {
                name: "content.txt".into(),
                data: EntryData::Text("hello".into()),
                category: FileCategory::Text,
            },
        ];
        let target = dir.path().join("out/Post.zip");
        let mut seen = 0;
        let (files, total) = build(&target, &entries, 0, |b| seen += b).unwrap();
        assert_eq!(files, vec![target.clone()]);
        assert_eq!(total, 2005);
        assert_eq!(seen, 2005);
        assert_eq!(names(&target), ["img.png", "img (1).png", "content.txt"]);
        let mut archive = zip::ZipArchive::new(File::open(&target).unwrap()).unwrap();
        let mut text = String::new();
        archive
            .by_name("content.txt")
            .unwrap()
            .read_to_string(&mut text)
            .unwrap();
        assert_eq!(text, "hello");

        let split = dir.path().join("Split.zip");
        let (files, _) = build(&split, &entries, 1500, |_| {}).unwrap();
        assert_eq!(files.len(), 2);
        assert_eq!(files[1], dir.path().join("Split (part 2).zip"));
        assert_eq!(names(&files[1]), ["img.png", "content.txt"]);
        assert!(!crate::jobs::transfer::part_path(&split).exists());
    }
}
