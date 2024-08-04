use std::{
    fmt::Display,
    fs::File,
    io::{Cursor, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::Arc,
    time::SystemTime,
};

use binrw::{BinReaderExt, Endian};
use destiny_pkg::{GameVersion, PackageManager, TagHash, TagHash64};
use eframe::epaint::mutex::RwLock;
use itertools::Itertools;
use log::{error, info, warn};
use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use rustc_hash::FxHashMap;

use crate::{
    package_manager::package_manager,
    text::create_stringmap,
    util::{u32_from_endian, u64_from_endian},
};

#[derive(bincode::Encode, bincode::Decode)]
pub struct TagCache {
    /// Timestamp of the packages directory
    pub timestamp: u64,

    pub version: u32,

    pub hashes: FxHashMap<TagHash, ScanResult>,
}

impl Default for TagCache {
    fn default() -> Self {
        Self {
            timestamp: 0,
            version: 5,
            hashes: Default::default(),
        }
    }
}

// Shareable read-only context
pub struct ScannerContext {
    pub valid_file_hashes: Vec<TagHash>,
    pub valid_file_hashes64: Vec<TagHash64>,
    pub known_string_hashes: Vec<u32>,
    pub endian: Endian,
}

#[derive(Clone, bincode::Encode, bincode::Decode, Debug)]
pub struct ScanResult {
    /// Were we able to read the tag data?
    pub successful: bool,

    pub file_hashes: Vec<ScannedHash<TagHash>>,
    pub file_hashes64: Vec<ScannedHash<TagHash64>>,
    pub string_hashes: Vec<ScannedHash<u32>>,
    pub raw_strings: Vec<String>,

    /// References from other files
    pub references: Vec<TagHash>,
}

impl Default for ScanResult {
    fn default() -> Self {
        ScanResult {
            successful: true,
            file_hashes: Default::default(),
            file_hashes64: Default::default(),
            string_hashes: Default::default(),
            raw_strings: Default::default(),
            references: Default::default(),
        }
    }
}

#[derive(Clone, bincode::Encode, bincode::Decode, Debug)]
pub struct ScannedHash<T: Sized + bincode::Encode + bincode::Decode> {
    pub offset: u64,
    pub hash: T,
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct ScannedArray {
    pub offset: u64,
    pub count: usize,
    pub class: u32,
}

pub const FNV1_BASE: u32 = 0x811c9dc5;
pub const FNV1_PRIME: u32 = 0x01000193;
pub fn fnv1(data: &[u8]) -> u32 {
    data.iter().fold(FNV1_BASE, |acc, b| {
        acc.wrapping_mul(FNV1_PRIME) ^ (*b as u32)
    })
}

pub fn scan_file(context: &ScannerContext, data: &[u8], tags_only: bool) -> ScanResult {
    profiling::scope!(
        "scan_file",
        format!("data len = {} bytes", data.len()).as_str()
    );

    let mut r = ScanResult::default();

    for offset in (0..data.len()).step_by(4) {
        if offset + 4 > data.len() {
            break;
        }
        let m: [u8; 4] = data[offset..offset + 4].try_into().unwrap();
        let value = u32_from_endian(context.endian, m);
        let hash = TagHash(value);

        if hash.is_pkg_file() && context.valid_file_hashes.binary_search(&hash).is_ok() {
            r.file_hashes.push(ScannedHash {
                offset: offset as u64,
                hash,
            });
        }

        if !tags_only {
            // cohae: 0x808000CB is used in the alpha
            if matches!(value, 0x80800065 | 0x808000CB) {
                r.raw_strings.extend(
                    read_raw_string_blob(data, offset as u64)
                        .into_iter()
                        .map(|(_, s)| s),
                );
            }
        }

        if value != 0x811c9dc5 && context.known_string_hashes.binary_search(&value).is_ok() {
            r.string_hashes.push(ScannedHash {
                offset: offset as u64,
                hash: value,
            });
        }

        if (offset % 8) == 0 && offset + 8 <= data.len() {
            let m: [u8; 8] = data[offset..offset + 8].try_into().unwrap();
            let value64 = u64_from_endian(context.endian, m);

            let hash = TagHash64(value64);
            {
                profiling::scope!("check 64 bit hash");
                if context.valid_file_hashes64.binary_search(&hash).is_ok() {
                    profiling::scope!("insert 64 bit hash");
                    r.file_hashes64.push(ScannedHash {
                        offset: offset as u64,
                        hash,
                    });
                }
            }
        }
    }

    r
}

#[profiling::function]
pub fn read_raw_string_blob(data: &[u8], offset: u64) -> Vec<(u64, String)> {
    let mut strings = vec![];

    let mut c = Cursor::new(data);
    (|| {
        c.seek(SeekFrom::Start(offset + 4))?;
        let (buffer_size, buffer_base_offset) = if matches!(
            package_manager().version,
            GameVersion::DestinyInternalAlpha | GameVersion::DestinyTheTakenKing
        ) {
            let buffer_size: u32 = c.read_be()?;
            let buffer_base_offset = offset + 4 + 4;
            (buffer_size as u64, buffer_base_offset)
        } else {
            let buffer_size: u64 = c.read_le()?;
            let buffer_base_offset = offset + 4 + 8;
            (buffer_size, buffer_base_offset)
        };

        let mut buffer = vec![0u8; buffer_size as usize];
        c.read_exact(&mut buffer)?;

        let mut s = String::new();
        let mut string_start = 0_u64;
        for (i, b) in buffer.into_iter().enumerate() {
            match b as char {
                '\0' => {
                    if !s.is_empty() {
                        strings.push((buffer_base_offset + string_start, s.clone()));
                        s.clear();
                    }

                    string_start = i as u64 + 1;
                }
                c => s.push(c),
            }
        }

        if !s.is_empty() {
            strings.push((buffer_base_offset + string_start, s));
        }

        <anyhow::Result<()>>::Ok(())
    })()
    .ok();

    strings
}

pub fn create_scanner_context(package_manager: &PackageManager) -> anyhow::Result<ScannerContext> {
    info!("Creating scanner context");

    // TODO(cohae): TTK PS4 is little endian
    let endian = package_manager.version.endian();

    let stringmap = create_stringmap()?;

    let mut res = ScannerContext {
        valid_file_hashes: package_manager
            .package_entry_index
            .iter()
            .flat_map(|(pkg_id, entries)| {
                entries
                    .iter()
                    .enumerate()
                    .map(|(entry_id, _)| TagHash::new(*pkg_id, entry_id as _))
                    .collect_vec()
            })
            .collect(),
        valid_file_hashes64: package_manager
            .hash64_table
            .keys()
            .map(|&v| TagHash64(v))
            .collect(),
        known_string_hashes: stringmap.keys().cloned().collect(),
        endian,
    };

    res.valid_file_hashes.sort_unstable();
    res.valid_file_hashes64.sort_unstable();
    res.known_string_hashes.sort_unstable();

    Ok(res)
}

#[derive(Copy, Clone)]
pub enum ScanStatus {
    None,
    CreatingScanner,
    Scanning {
        current_package: usize,
        total_packages: usize,
    },
    TransformGathering,
    TransformApplying,
    WritingCache,
    LoadingCache,
}

impl Display for ScanStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScanStatus::None => Ok(()),
            ScanStatus::CreatingScanner => f.write_str("Creating scanner"),
            ScanStatus::Scanning {
                current_package,
                total_packages,
            } => f.write_fmt(format_args!(
                "Creating new cache {}/{}",
                current_package, total_packages
            )),
            ScanStatus::TransformGathering => {
                f.write_str("Transforming cache (gathering references)")
            }
            ScanStatus::TransformApplying => {
                f.write_str("Transforming cache (applying references)")
            }
            ScanStatus::WritingCache => f.write_str("Writing cache"),
            ScanStatus::LoadingCache => f.write_str("Loading cache"),
        }
    }
}

lazy_static::lazy_static! {
    static ref SCANNER_PROGRESS: RwLock<ScanStatus> = RwLock::new(ScanStatus::None);
}

pub fn scanner_progress() -> ScanStatus {
    *SCANNER_PROGRESS.read()
}

pub fn load_tag_cache() -> TagCache {
    let cache_name = format!("tags_{}.cache", package_manager().cache_key());
    let cache_file_path = exe_relative_path(&cache_name);

    if let Ok(cache_file) = File::open(&cache_file_path) {
        info!("Existing cache file found, loading");
        *SCANNER_PROGRESS.write() = ScanStatus::LoadingCache;

        let cache_data = zstd::Decoder::new(cache_file).and_then(|mut r| {
            let mut buf = vec![];
            r.read_to_end(&mut buf)?;
            Ok(buf)
        });

        match cache_data {
            Ok(cache_data) => {
                if let Ok((cache, _)) = bincode::decode_from_slice::<TagCache, _>(
                    &cache_data,
                    bincode::config::standard(),
                ) {
                    match cache.version.cmp(&TagCache::default().version) {
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
                            } else {
                                *SCANNER_PROGRESS.write() = ScanStatus::None;
                                return cache;
                            }
                        }
                        std::cmp::Ordering::Less => {
                            info!(
                                "Cache is out of date, rebuilding (cache: {}, quicktag: {})",
                                cache.version,
                                TagCache::default().version
                            );
                        }
                        std::cmp::Ordering::Greater => {
                            error!("Tried to open a future version cache with an old quicktag version (cache: {}, quicktag: {})",
                                cache.version,
                                TagCache::default().version
                            );

                            native_dialog::MessageDialog::new()
                                .set_type(native_dialog::MessageType::Error)
                                .set_title("Future cache")
                                .set_text(&format!("Your cache file ({cache_name}) is newer than this build of quicktag\n\nCache version: v{}\nExpected version: v{}", cache.version, TagCache::default().version))
                                .show_alert()
                                .unwrap();

                            std::process::exit(21);
                        }
                    }
                } else {
                    warn!("Cache file is invalid, creating a new one");
                }
            }
            Err(e) => error!("Cache file is invalid: {e}"),
        }
    }

    *SCANNER_PROGRESS.write() = ScanStatus::CreatingScanner;
    let scanner_context = Arc::new(
        create_scanner_context(&package_manager()).expect("Failed to create scanner context"),
    );

    let all_pkgs = package_manager()
        .package_paths
        .values()
        .cloned()
        .collect_vec();

    let version = package_manager().version;
    let package_count = all_pkgs.len();
    let cache: FxHashMap<TagHash, ScanResult> = all_pkgs
        .par_iter()
        .map_with(scanner_context, |context, path| {
            profiling::scope!("scan_pkg", &path.path);
            let current_package = {
                let mut p = SCANNER_PROGRESS.write();
                let current_package = if let ScanStatus::Scanning {
                    current_package, ..
                } = *p
                {
                    current_package
                } else {
                    0
                };

                *p = ScanStatus::Scanning {
                    current_package: current_package + 1,
                    total_packages: package_count,
                };

                current_package
            };

            info!("Opening pkg {path} ({}/{package_count})", current_package);
            let pkg = {
                profiling::scope!("open package");
                version.open(&path.path).unwrap()
            };

            let mut all_tags = match version {
                GameVersion::DestinyInternalAlpha => [
                    pkg.get_all_by_type(16, None),
                    pkg.get_all_by_type(128, None),
                ]
                .concat(),
                GameVersion::DestinyRiseOfIron | GameVersion::DestinyTheTakenKing => [
                    pkg.get_all_by_type(16, None),
                    pkg.get_all_by_type(128, None),
                ]
                .concat(),
                GameVersion::Destiny2Beta
                | GameVersion::Destiny2Forsaken
                | GameVersion::Destiny2Shadowkeep
                | GameVersion::Destiny2BeyondLight
                | GameVersion::Destiny2WitchQueen
                | GameVersion::Destiny2Lightfall
                | GameVersion::Destiny2TheFinalShape => {
                    [pkg.get_all_by_type(8, None), pkg.get_all_by_type(16, None)].concat()
                }
            };

            // Sort tags by starting block index to optimize sequential block reads
            all_tags.sort_by_key(|v| v.1.starting_block);

            let mut results = FxHashMap::default();
            for (t, _) in all_tags {
                let hash = TagHash::new(pkg.pkg_id(), t as u16);
                profiling::scope!("scan_tag", format!("tag {hash}").as_str());

                let data = match pkg.read_entry(t) {
                    Ok(d) => d,
                    Err(e) => {
                        error!("Failed to read entry {path}:{t}: {e}");
                        results.insert(
                            hash,
                            ScanResult {
                                successful: false,
                                ..Default::default()
                            },
                        );
                        continue;
                    }
                };

                let mut scan_result = scan_file(context, &data, false);
                if version.is_d1() {
                    if let Some(entry) = pkg.entry(t) {
                        let ref_tag = TagHash(entry.reference);
                        if context.valid_file_hashes.contains(&ref_tag) {
                            scan_result.file_hashes.insert(
                                0,
                                ScannedHash {
                                    offset: u64::MAX,
                                    hash: ref_tag,
                                },
                            );
                        }
                    }
                }
                results.insert(hash, scan_result);
            }

            results
        })
        .flatten()
        .collect();

    let cache = transform_tag_cache(cache);

    *SCANNER_PROGRESS.write() = ScanStatus::WritingCache;
    info!("Compressing tag cache...");
    let mut writer = zstd::Encoder::new(File::create(cache_file_path).unwrap(), 3).unwrap();

    bincode::encode_into_std_write(&cache, &mut writer, bincode::config::standard()).unwrap();
    writer.finish().unwrap();
    *SCANNER_PROGRESS.write() = ScanStatus::None;

    cache
}

/// Transforms the tag cache to include reference lookup tables
fn transform_tag_cache(cache: FxHashMap<TagHash, ScanResult>) -> TagCache {
    info!("Transforming tag cache...");

    let mut new_cache: TagCache = Default::default();

    *SCANNER_PROGRESS.write() = ScanStatus::TransformGathering;
    info!("\t- Gathering references");
    let mut direct_reference_cache: FxHashMap<TagHash, Vec<TagHash>> = Default::default();
    for (k2, v2) in &cache {
        for t32 in &v2.file_hashes {
            match direct_reference_cache.entry(t32.hash) {
                std::collections::hash_map::Entry::Occupied(mut o) => {
                    o.get_mut().push(*k2);
                }
                std::collections::hash_map::Entry::Vacant(v) => {
                    v.insert(vec![*k2]);
                }
            }
        }

        for t64 in &v2.file_hashes64 {
            if let Some(t32) = package_manager().hash64_table.get(&t64.hash.0) {
                match direct_reference_cache.entry(t32.hash32) {
                    std::collections::hash_map::Entry::Occupied(mut o) => {
                        o.get_mut().push(*k2);
                    }
                    std::collections::hash_map::Entry::Vacant(v) => {
                        v.insert(vec![*k2]);
                    }
                }
            }
        }
    }

    *SCANNER_PROGRESS.write() = ScanStatus::TransformApplying;
    info!("\t- Applying references");
    for (k, v) in &cache {
        let mut scan = v.clone();

        if let Some(refs) = direct_reference_cache.get(k) {
            scan.references = refs.clone();
        }

        new_cache.hashes.insert(*k, scan);
    }

    info!("\t- Adding remaining non-structure tags");
    for (k, v) in direct_reference_cache {
        if !v.is_empty() && !new_cache.hashes.contains_key(&k) {
            new_cache.hashes.insert(
                k,
                ScanResult {
                    references: v,
                    ..Default::default()
                },
            );
        }
    }

    let timestamp = std::fs::metadata(&package_manager().package_dir)
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

    new_cache.timestamp = timestamp;

    new_cache
}

fn exe_directory() -> PathBuf {
    std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn exe_relative_path<P: AsRef<Path>>(path: P) -> PathBuf {
    exe_directory().join(path.as_ref())
}
