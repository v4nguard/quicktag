use std::{fs::File, io::Read, path::Path, time::SystemTime};

use super::ScanResult;

use log::{error, info, warn};
use rustc_hash::FxHashMap;
use tiger_pkg::{TagHash, package_manager};

#[derive(bincode::Encode, bincode::Decode)]
pub struct TagCache {
    /// Timestamp of the packages directory
    pub timestamp: u64,

    pub version: u32,

    pub hashes: FxHashMap<TagHash, ScanResult>,
}

impl TagCache {
    pub const VERSION: u32 = 8;

    pub fn load(path: impl AsRef<Path>) -> anyhow::Result<CacheLoadResult> {
        if let Ok(cache_file) = File::open(&path) {
            info!("Existing cache file found, loading");

            let cache_data = zstd::Decoder::new(cache_file).and_then(|mut r| {
                let mut buf = vec![];
                r.read_to_end(&mut buf)?;
                Ok(buf)
            });

            match cache_data {
                Ok(cache_data) => {
                    if let Ok((cache, _)) = bincode::decode_from_slice::<Self, _>(
                        &cache_data,
                        bincode::config::standard(),
                    ) {
                        match cache.version.cmp(&Self::VERSION) {
                            std::cmp::Ordering::Equal => {
                                let current_pkg_timestamp =
                                    std::fs::metadata(&package_manager().package_dir)
                                        .ok()
                                        .and_then(|m| {
                                            Some(
                                                m.modified()
                                                    .ok()?
                                                    .duration_since(SystemTime::UNIX_EPOCH)
                                                    .ok()?
                                                    .as_secs(),
                                            )
                                        })
                                        .unwrap_or(0);

                                if cache.timestamp < current_pkg_timestamp {
                                    info!(
                                        "Cache is out of date, rebuilding (cache: {}, package dir: {})",
                                        chrono::DateTime::from_timestamp(cache.timestamp as i64, 0)
                                            .unwrap()
                                            .format("%Y-%m-%d"),
                                        chrono::DateTime::from_timestamp(
                                            current_pkg_timestamp as i64,
                                            0
                                        )
                                        .unwrap()
                                        .format("%Y-%m-%d"),
                                    );

                                    Ok(CacheLoadResult::Rebuild)
                                } else {
                                    Ok(CacheLoadResult::Loaded(cache))
                                }
                            }
                            std::cmp::Ordering::Less => {
                                info!(
                                    "Cache is out of date, rebuilding (cache: {}, quicktag: {})",
                                    cache.version,
                                    TagCache::default().version
                                );
                                Ok(CacheLoadResult::Rebuild)
                            }
                            std::cmp::Ordering::Greater => {
                                error!(
                                    "Tried to open a future version cache with an old quicktag version (cache: {}, quicktag: {})",
                                    cache.version,
                                    TagCache::default().version
                                );

                                native_dialog::MessageDialog::new()
                                    .set_type(native_dialog::MessageType::Error)
                                    .set_title("Future cache")
                                    .set_text(&format!("Your cache file ({}) is newer than this build of quicktag\n\nCache version: v{}\nExpected version: v{}", path.as_ref().display(), cache.version, Self::default().version))
                                    .show_alert()
                                    .unwrap();

                                std::process::exit(21);
                            }
                        }
                    } else {
                        warn!("Cache file is corrupt, creating a new one");
                        Ok(CacheLoadResult::Rebuild)
                    }
                }
                Err(e) => {
                    error!("Failed to load cache file, creating a new one: {e}");
                    Ok(CacheLoadResult::Rebuild)
                }
            }
        } else {
            Ok(CacheLoadResult::Rebuild)
        }
    }
}

impl Default for TagCache {
    fn default() -> Self {
        Self {
            timestamp: 0,
            version: Self::VERSION,
            hashes: Default::default(),
        }
    }
}

pub enum CacheLoadResult {
    Loaded(TagCache),
    Rebuild,
}
