//! Mega share links. Files on Mega are encrypted (AES-128); the key is the
//! part of the link after `#`, which browsers never send to Mega. The app
//! reads file names and download addresses from Mega's public API, downloads
//! the encrypted bytes and decrypts them on this computer.

use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

use aes::cipher::{BlockDecrypt, BlockDecryptMut, KeyInit, KeyIvInit, StreamCipher};
use aes::Aes128;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use super::{HostError, HostFile};

const API: &str = "https://g.api.mega.co.nz/cs";

/// What a Mega link points at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MegaLink {
    File {
        id: String,
        key: String,
    },
    Folder {
        id: String,
        key: String,
        file: Option<String>,
    },
}

/// Key and nonce to decrypt one file (hex, so it can be saved with a job).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileKey {
    pub key: String,
    pub iv: String,
}

/// Reads `mega.nz/file/ID#KEY`, `mega.nz/folder/ID#KEY[/file/ID]` and the
/// old `#!ID!KEY` / `#F!ID!KEY` forms.
pub fn parse_link(link: &str) -> Option<MegaLink> {
    let url = url::Url::parse(link).ok()?;
    let fragment = url.fragment().unwrap_or("");
    let parts: Vec<&str> = url.path().split('/').filter(|p| !p.is_empty()).collect();
    match parts.as_slice() {
        ["file", id] if !fragment.is_empty() => {
            return Some(MegaLink::File {
                id: id.to_string(),
                key: fragment.to_string(),
            })
        }
        ["folder", id] if !fragment.is_empty() => {
            let (key, file) = match fragment.split_once("/file/") {
                Some((k, f)) => (k.to_string(), Some(f.to_string())),
                None => (fragment.split('/').next().unwrap_or("").to_string(), None),
            };
            return Some(MegaLink::Folder {
                id: id.to_string(),
                key,
                file,
            });
        }
        _ => {}
    }
    // Old style: #!id!key and #F!id!key.
    let mut old = fragment.split('!');
    match (old.next(), old.next(), old.next()) {
        (Some(""), Some(id), Some(key)) => Some(MegaLink::File {
            id: id.into(),
            key: key.into(),
        }),
        (Some("F"), Some(id), Some(key)) => Some(MegaLink::Folder {
            id: id.into(),
            key: key.into(),
            file: None,
        }),
        _ => None,
    }
}

fn b64(s: &str) -> Option<Vec<u8>> {
    URL_SAFE_NO_PAD
        .decode(
            s.trim()
                .trim_end_matches('=')
                .replace('+', "-")
                .replace('/', "_"),
        )
        .ok()
}

fn words(bytes: &[u8]) -> Vec<u32> {
    bytes
        .chunks(4)
        .map(|c| {
            let mut b = [0u8; 4];
            b[..c.len()].copy_from_slice(c);
            u32::from_be_bytes(b)
        })
        .collect()
}

fn bytes_of(words: &[u32]) -> Vec<u8> {
    words.iter().flat_map(|w| w.to_be_bytes()).collect()
}

/// A 32-byte file key folds into the AES key and the CTR nonce.
fn file_key(raw: &[u8]) -> Option<([u8; 16], [u8; 16])> {
    if raw.len() != 32 {
        return None;
    }
    let a = words(raw);
    let key = bytes_of(&[a[0] ^ a[4], a[1] ^ a[5], a[2] ^ a[6], a[3] ^ a[7]]);
    let iv = bytes_of(&[a[4], a[5], 0, 0]);
    Some((key.try_into().ok()?, iv.try_into().ok()?))
}

/// Node keys in a shared folder are encrypted with the folder key (AES-ECB).
fn decrypt_node_key(encrypted: &[u8], folder_key: &[u8; 16]) -> Vec<u8> {
    let cipher = Aes128::new(folder_key.into());
    let mut out = encrypted.to_vec();
    for block in out.chunks_exact_mut(16) {
        cipher.decrypt_block(block.into());
    }
    out
}

/// File names are stored as `MEGA{"n":"name"}`, AES-CBC encrypted with a zero IV.
fn decrypt_name(attributes: &str, key: &[u8; 16]) -> Option<String> {
    let mut data = b64(attributes)?;
    if data.is_empty() || data.len() % 16 != 0 {
        return None;
    }
    let decryptor = cbc::Decryptor::<Aes128>::new(key.into(), &[0u8; 16].into());
    let plain = decryptor
        .decrypt_padded_mut::<aes::cipher::block_padding::NoPadding>(&mut data)
        .ok()?;
    let text = std::str::from_utf8(plain).ok()?.trim_end_matches('\0');
    let json = text.strip_prefix("MEGA")?;
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    value["n"].as_str().map(str::to_string)
}

async fn call(
    http: &reqwest::Client,
    folder: Option<&str>,
    body: serde_json::Value,
    cancel: &CancellationToken,
) -> Result<serde_json::Value, HostError> {
    let seq: u32 = rand::random();
    let mut url = format!("{API}?id={seq}");
    if let Some(f) = folder {
        url.push_str("&n=");
        url.push_str(f);
    }
    let request = http
        .post(url)
        .header("Content-Type", "application/json")
        .body(serde_json::Value::Array(vec![body]).to_string());
    let value = super::json(request, cancel).await?;
    // Errors come back as negative numbers, alone or in the array.
    let first = match &value {
        serde_json::Value::Array(items) => items.first().cloned().unwrap_or_default(),
        other => other.clone(),
    };
    if let Some(code) = first.as_i64() {
        return Err(match code {
            -9 | -2 => HostError::NotFound,
            -11 | -16 | -17 => HostError::Refused("access denied".into()),
            -3 | -4 | -6 => HostError::Refused("Mega is busy, try later".into()),
            other => HostError::Refused(format!("Mega error {other}")),
        });
    }
    Ok(first)
}

pub async fn resolve(
    http: &reqwest::Client,
    link: &str,
    cancel: &CancellationToken,
) -> Result<Vec<HostFile>, HostError> {
    match parse_link(link).ok_or(HostError::Unsupported)? {
        MegaLink::File { id, key } => {
            let raw = b64(&key).ok_or(HostError::Unsupported)?;
            let (aes_key, iv) = file_key(&raw).ok_or(HostError::Unsupported)?;
            let info = call(
                http,
                None,
                serde_json::json!({"a": "g", "g": 1, "p": id}),
                cancel,
            )
            .await?;
            let url = info["g"].as_str().ok_or(HostError::NotFound)?.to_string();
            let name = info["at"]
                .as_str()
                .and_then(|at| decrypt_name(at, &aes_key))
                .unwrap_or_else(|| id.clone());
            Ok(vec![HostFile {
                name,
                url,
                size: info["s"].as_u64(),
                folder: String::new(),
                headers: vec![],
                mega: Some(FileKey {
                    key: hex::encode(aes_key),
                    iv: hex::encode(iv),
                }),
            }])
        }
        MegaLink::Folder { id, key, file } => {
            let folder_key: [u8; 16] = b64(&key)
                .and_then(|k| k.try_into().ok())
                .ok_or(HostError::Unsupported)?;
            let listing = call(
                http,
                Some(&id),
                serde_json::json!({"a": "f", "c": 1, "r": 1, "ca": 1}),
                cancel,
            )
            .await?;
            let nodes = listing["f"].as_array().cloned().unwrap_or_default();
            // Folder names, to rebuild each file's path.
            let mut names = std::collections::HashMap::new();
            let mut parents = std::collections::HashMap::new();
            let mut files = Vec::new();
            for n in &nodes {
                let handle = n["h"].as_str().unwrap_or_default().to_string();
                parents.insert(
                    handle.clone(),
                    n["p"].as_str().unwrap_or_default().to_string(),
                );
                let Some(enc) = n["k"]
                    .as_str()
                    .and_then(|k| k.split('/').next())
                    .and_then(|k| k.split_once(':').map(|(_, v)| v))
                    .and_then(b64)
                else {
                    continue;
                };
                let node_key = decrypt_node_key(&enc, &folder_key);
                match n["t"].as_i64() {
                    Some(1) => {
                        let k: Option<[u8; 16]> =
                            node_key.get(..16).and_then(|k| k.try_into().ok());
                        if let (Some(k), Some(at)) = (k, n["a"].as_str()) {
                            if let Some(name) = decrypt_name(at, &k) {
                                names.insert(handle, name);
                            }
                        }
                    }
                    Some(0) => {
                        if let Some((aes_key, iv)) = file_key(&node_key) {
                            let name = n["a"]
                                .as_str()
                                .and_then(|at| decrypt_name(at, &aes_key))
                                .unwrap_or_else(|| handle.clone());
                            files.push((handle, name, n["s"].as_u64(), aes_key, iv));
                        }
                    }
                    _ => {}
                }
            }
            let path_of = |handle: &str| {
                let mut parts = Vec::new();
                let mut current = parents.get(handle).cloned().unwrap_or_default();
                let mut guard = 0;
                while let Some(name) = names.get(&current) {
                    parts.push(name.clone());
                    current = parents.get(&current).cloned().unwrap_or_default();
                    guard += 1;
                    if guard > 32 {
                        break;
                    }
                }
                // The shared folder's own name is the top; keep what is below it.
                parts.pop();
                parts.reverse();
                parts.join("/")
            };
            let mut out = Vec::new();
            for (handle, name, size, aes_key, iv) in files {
                if file.as_ref().is_some_and(|f| *f != handle) {
                    continue;
                }
                let info = call(
                    http,
                    Some(&id),
                    serde_json::json!({"a": "g", "g": 1, "n": handle}),
                    cancel,
                )
                .await?;
                let Some(url) = info["g"].as_str() else {
                    continue;
                };
                out.push(HostFile {
                    folder: path_of(&handle),
                    name,
                    url: url.to_string(),
                    size,
                    headers: vec![],
                    mega: Some(FileKey {
                        key: hex::encode(aes_key),
                        iv: hex::encode(iv),
                    }),
                });
                if out.len() >= 2000 {
                    break;
                }
            }
            Ok(out)
        }
    }
}

/// Decrypts a downloaded Mega file in place (AES-128-CTR).
pub fn decrypt_file(path: &Path, key: &FileKey) -> std::io::Result<()> {
    let bad = |_| std::io::Error::new(std::io::ErrorKind::InvalidData, "bad Mega key");
    let k: [u8; 16] = hex::decode(&key.key)
        .map_err(bad)?
        .try_into()
        .map_err(|_| bad(hex::FromHexError::InvalidStringLength))?;
    let iv: [u8; 16] = hex::decode(&key.iv)
        .map_err(bad)?
        .try_into()
        .map_err(|_| bad(hex::FromHexError::InvalidStringLength))?;
    let mut cipher = ctr::Ctr128BE::<Aes128>::new(&k.into(), &iv.into());
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)?;
    let mut buf = vec![0u8; 1 << 20];
    let mut pos = 0u64;
    loop {
        file.seek(SeekFrom::Start(pos))?;
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        cipher.apply_keystream(&mut buf[..n]);
        file.seek(SeekFrom::Start(pos))?;
        file.write_all(&buf[..n])?;
        pos += n as u64;
    }
    file.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use aes::cipher::{BlockEncrypt, BlockEncryptMut};

    #[test]
    fn parses_new_and_old_links() {
        assert_eq!(
            parse_link("https://mega.nz/file/AbCd#KeY"),
            Some(MegaLink::File {
                id: "AbCd".into(),
                key: "KeY".into()
            })
        );
        assert_eq!(
            parse_link("https://mega.nz/folder/F1#FK/file/X9"),
            Some(MegaLink::Folder {
                id: "F1".into(),
                key: "FK".into(),
                file: Some("X9".into())
            })
        );
        assert_eq!(
            parse_link("https://mega.nz/#!AbCd!KeY"),
            Some(MegaLink::File {
                id: "AbCd".into(),
                key: "KeY".into()
            })
        );
        assert_eq!(
            parse_link("https://mega.co.nz/#F!F1!FK"),
            Some(MegaLink::Folder {
                id: "F1".into(),
                key: "FK".into(),
                file: None
            })
        );
        assert_eq!(parse_link("https://mega.nz/file/AbCd"), None);
    }

    /// Encrypts like Mega does, so the decrypting side is checked against
    /// the real scheme rather than against itself.
    fn mega_name(name: &str, key: &[u8; 16]) -> String {
        let mut plain = format!("MEGA{{\"n\":\"{name}\"}}").into_bytes();
        while plain.len() % 16 != 0 {
            plain.push(0);
        }
        let len = plain.len();
        let enc = cbc::Encryptor::<Aes128>::new(key.into(), &[0u8; 16].into());
        let out = enc
            .encrypt_padded_mut::<aes::cipher::block_padding::NoPadding>(&mut plain, len)
            .unwrap()
            .to_vec();
        URL_SAFE_NO_PAD.encode(out)
    }

    #[test]
    fn derives_keys_and_reads_names() {
        let raw: Vec<u8> = (0u8..32).collect();
        let (key, iv) = file_key(&raw).unwrap();
        let a = words(&raw);
        assert_eq!(&key[..4], &(a[0] ^ a[4]).to_be_bytes());
        assert_eq!(&iv[..8], &raw[16..24]);
        assert_eq!(&iv[8..], &[0u8; 8]);
        assert_eq!(
            decrypt_name(&mega_name("Set 1.zip", &key), &key).as_deref(),
            Some("Set 1.zip")
        );
        assert_eq!(decrypt_name(&mega_name("x", &key), &[9u8; 16]), None);
        // Folder node keys come back with the folder key.
        let folder_key = [7u8; 16];
        let cipher = Aes128::new(&folder_key.into());
        let mut enc = raw.clone();
        for block in enc.chunks_exact_mut(16) {
            cipher.encrypt_block(block.into());
        }
        assert_eq!(decrypt_node_key(&enc, &folder_key), raw);
    }

    #[test]
    fn decrypts_files_in_place() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f.bin");
        let plain: Vec<u8> = (0..3_000_000u32).map(|i| (i % 251) as u8).collect();
        let k = [3u8; 16];
        let iv = [5u8, 6, 7, 8, 9, 10, 11, 12, 0, 0, 0, 0, 0, 0, 0, 0];
        let mut enc = plain.clone();
        ctr::Ctr128BE::<Aes128>::new(&k.into(), &iv.into()).apply_keystream(&mut enc);
        std::fs::write(&path, &enc).unwrap();
        decrypt_file(
            &path,
            &FileKey {
                key: hex::encode(k),
                iv: hex::encode(iv),
            },
        )
        .unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), plain);
    }
}
