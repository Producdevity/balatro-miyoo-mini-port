// Derived from balatro-port-tui (Apache-2.0). Modified by Producdevity; see NOTICE.

use crate::miyoo::patches::{patch_controller_input, patch_miyoo_script};
use parking_lot::Mutex;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Reads game files from a directory or ZIP archive.
pub enum GameSource {
    Directory(PathBuf),
    Zip {
        base_dir: String,
        archive: Mutex<zip::ZipArchive<std::fs::File>>,
        files: HashSet<String>,
    },
}

impl GameSource {
    pub fn from_path(path: &Path) -> anyhow::Result<Self> {
        if path.is_dir() {
            return Ok(GameSource::Directory(path.to_owned()));
        }
        // Try opening as zip (handles Balatro.exe = zip-appended)
        let file = std::fs::File::open(path)?;
        let mut archive = zip::ZipArchive::new(file)?;
        let mut files = HashSet::new();
        let count = archive.len();
        eprintln!("[filesystem] indexing {count} archive entries");
        for i in 0..count {
            if let Ok(entry) = archive.by_index(i) {
                let name = entry.name().replace('\\', "/");
                if !entry.is_dir() {
                    files.insert(name);
                }
            }
        }
        let base_dir = path
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        Ok(GameSource::Zip {
            base_dir,
            archive: Mutex::new(archive),
            files,
        })
    }

    pub fn read_file(&self, file_path: &str) -> anyhow::Result<Vec<u8>> {
        if std::env::var("BALATRO_PLATFORM").as_deref() == Ok("miyoo")
            && file_path.replace('\\', "/") == "resources/fonts/m6x11plus.ttf"
        {
            return Ok(include_bytes!("../assets/Nunito-Black.ttf").to_vec());
        }
        let mut data = match self {
            GameSource::Directory(base) => {
                let full = base.join(file_path);
                std::fs::read(full)?
            }
            GameSource::Zip { archive, files, .. } => {
                let normalized = file_path.replace('\\', "/");
                let name = if files.contains(&normalized) {
                    normalized.as_str()
                } else if files.contains(file_path) {
                    file_path
                } else {
                    anyhow::bail!("file not found in archive: {}", file_path)
                };
                let mut archive = archive.lock();
                let mut entry = archive.by_name(name)?;
                let mut data = Vec::with_capacity(entry.size() as usize);
                std::io::Read::read_to_end(&mut entry, &mut data)?;
                data
            }
        };
        if file_path.replace('\\', "/") == "engine/controller.lua" {
            patch_controller_input(&mut data);
        }
        if std::env::var_os("BALATRO_PLATFORM").as_deref() == Some(std::ffi::OsStr::new("miyoo")) {
            patch_miyoo_script(file_path, &mut data);
        }
        Ok(data)
    }

    pub fn file_exists(&self, file_path: &str) -> bool {
        match self {
            GameSource::Directory(base) => base.join(file_path).exists(),
            GameSource::Zip { files, .. } => {
                let normalized = file_path.replace('\\', "/");
                files.contains(&normalized) || files.contains(file_path)
            }
        }
    }

    pub fn is_directory(&self, dir_path: &str) -> bool {
        match self {
            GameSource::Directory(base) => base.join(dir_path).is_dir(),
            GameSource::Zip { files, .. } => {
                let prefix = if dir_path.ends_with('/') {
                    dir_path.replace('\\', "/")
                } else {
                    format!("{}/", dir_path.replace('\\', "/"))
                };
                files.iter().any(|k| k.starts_with(&prefix))
            }
        }
    }

    pub fn list_directory(&self, dir_path: &str) -> Vec<String> {
        match self {
            GameSource::Directory(base) => {
                let full = base.join(dir_path);
                std::fs::read_dir(full)
                    .map(|entries| {
                        entries
                            .filter_map(|e| e.ok())
                            .map(|e| e.file_name().to_string_lossy().into_owned())
                            .collect()
                    })
                    .unwrap_or_default()
            }
            GameSource::Zip { files, .. } => {
                let prefix = if dir_path.is_empty() {
                    String::new()
                } else if dir_path.ends_with('/') {
                    dir_path.replace('\\', "/")
                } else {
                    format!("{}/", dir_path.replace('\\', "/"))
                };
                let mut items = HashSet::new();
                for key in files.iter() {
                    if key.starts_with(&prefix) {
                        let rest = &key[prefix.len()..];
                        if let Some(slash) = rest.find('/') {
                            items.insert(rest[..slash].to_string());
                        } else if !rest.is_empty() {
                            items.insert(rest.to_string());
                        }
                    }
                }
                items.into_iter().collect()
            }
        }
    }

    pub fn source_base_directory(&self) -> String {
        match self {
            GameSource::Directory(base) => base.to_string_lossy().into_owned(),
            GameSource::Zip { base_dir, .. } => base_dir.clone(),
        }
    }
}
