//! Unpacking downloaded ZIP archives next to themselves, trying the
//! passwords found in the post when an archive is encrypted.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Stops archives that unpack to absurd sizes (zip bombs).
const MAX_TOTAL: u64 = 64 * 1024 * 1024 * 1024;
const MAX_ENTRIES: usize = 200_000;

#[derive(Debug, PartialEq, Eq)]
pub enum Extracted {
    /// Unpacked into this folder.
    Done(PathBuf),
    /// Encrypted, and none of the known passwords opened it.
    NeedsPassword,
}

/// The folder an archive unpacks into: its name without `.zip`.
pub fn folder_for(archive: &Path) -> PathBuf {
    let stem = archive
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "archive".into());
    archive.with_file_name(stem)
}

fn open_entry<'a>(
    zip: &'a mut zip::ZipArchive<fs::File>,
    i: usize,
    password: Option<&str>,
) -> zip::result::ZipResult<zip::read::ZipFile<'a, fs::File>> {
    match password {
        Some(p) => zip.by_index_decrypt(i, p.as_bytes()),
        None => zip.by_index(i),
    }
}

/// Unpacks `archive` into `folder_for(archive)`. Entries can never land
/// outside that folder. Blocking: run it off the async threads.
pub fn extract_zip(archive: &Path, passwords: &[String]) -> io::Result<Extracted> {
    let file = fs::File::open(archive)?;
    let mut zip = zip::ZipArchive::new(file).map_err(io::Error::other)?;
    if zip.len() > MAX_ENTRIES {
        return Err(io::Error::other("too many entries"));
    }
    // Find a password that opens the first encrypted entry.
    let mut password: Option<String> = None;
    let encrypted =
        (0..zip.len()).find(|&i| zip.by_index_raw(i).map(|f| f.encrypted()).unwrap_or(false));
    if let Some(i) = encrypted {
        let found = passwords.iter().find(|p| {
            open_entry(&mut zip, i, Some(p.as_str()))
                .and_then(|mut f| {
                    io::copy(&mut f, &mut io::sink()).map_err(zip::result::ZipError::Io)
                })
                .is_ok()
        });
        match found {
            Some(p) => password = Some(p.clone()),
            None => return Ok(Extracted::NeedsPassword),
        }
    }
    let target = folder_for(archive);
    fs::create_dir_all(&target)?;
    let mut total = 0u64;
    for i in 0..zip.len() {
        let mut entry = open_entry(&mut zip, i, password.as_deref()).map_err(io::Error::other)?;
        let Some(relative) = entry.enclosed_name() else {
            continue;
        };
        let out = target.join(relative);
        if !out.starts_with(&target) {
            continue;
        }
        if entry.is_dir() {
            fs::create_dir_all(&out)?;
            continue;
        }
        total = total.saturating_add(entry.size());
        if total > MAX_TOTAL {
            return Err(io::Error::other("the archive unpacks to too much"));
        }
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut writer = fs::File::create(&out)?;
        io::copy(&mut entry, &mut writer)?;
    }
    Ok(Extracted::Done(target))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    fn make(path: &Path, password: Option<&str>) {
        let mut w = zip::ZipWriter::new(fs::File::create(path).unwrap());
        let mut opts = SimpleFileOptions::default();
        if let Some(p) = password {
            opts = opts.with_aes_encryption(zip::AesMode::Aes256, p);
        }
        w.start_file("art/a.txt", opts).unwrap();
        w.write_all(b"hello").unwrap();
        w.start_file("../evil.txt", opts).unwrap();
        w.write_all(b"nope").unwrap();
        w.finish().unwrap();
    }

    #[test]
    fn unpacks_safely_and_tries_passwords() {
        let dir = tempfile::tempdir().unwrap();
        let plain = dir.path().join("Set 1.zip");
        make(&plain, None);
        assert_eq!(
            extract_zip(&plain, &[]).unwrap(),
            Extracted::Done(dir.path().join("Set 1"))
        );
        assert_eq!(
            fs::read(dir.path().join("Set 1/art/a.txt")).unwrap(),
            b"hello"
        );
        assert!(!dir.path().join("evil.txt").exists());

        let locked = dir.path().join("Locked.zip");
        make(&locked, Some("kitty"));
        assert_eq!(
            extract_zip(&locked, &["wrong".into()]).unwrap(),
            Extracted::NeedsPassword
        );
        assert!(matches!(
            extract_zip(&locked, &["wrong".into(), "kitty".into()]).unwrap(),
            Extracted::Done(_)
        ));
        assert_eq!(
            fs::read(dir.path().join("Locked/art/a.txt")).unwrap(),
            b"hello"
        );
    }
}
