pub mod cache;
pub mod context;

pub use cache::TagCache;

use std::{
    fmt::Display,
    fs::File,
    io::{Cursor, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::Arc,
    time::SystemTime,
};

use binrw::BinReaderExt;
use cache::CacheLoadResult;
use context::ScannerContext;
use itertools::Itertools;
use log::{error, info};
use parking_lot::RwLock;
use quicktag_core::{
    classes::get_class_by_id,
    tagtypes::TagType,
    util::{u32_from_endian, u64_from_endian},
};
use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use rustc_hash::FxHashMap;
use tiger_pkg::{
    DestinyVersion, GameVersion, TagHash, TagHash64, Version, package::UEntryHeader,
    package_manager,
};

#[derive(Clone, bincode::Encode, bincode::Decode, Debug)]
pub struct ScanResult {
    /// Were we able to read the tag data?
    pub successful: bool,

    pub file_hashes: Vec<ScannedHash<TagHash>>,
    pub file_hashes64: Vec<ScannedHash<TagHash64>>,
    pub string_hashes: Vec<ScannedHash<u32>>,
    pub wordlist_hashes: Vec<ScannedHash<u32>>,
    pub raw_strings: Vec<String>,

    /// References from other files
    pub references: Vec<TagHash>,

    pub secondary_class: Option<u32>,
}

impl Default for ScanResult {
    fn default() -> Self {
        ScanResult {
            successful: true,
            file_hashes: Default::default(),
            file_hashes64: Default::default(),
            string_hashes: Default::default(),
            wordlist_hashes: Default::default(),
            raw_strings: Default::default(),
            references: Default::default(),
            secondary_class: None,
        }
    }
}

#[derive(Clone, bincode::Encode, bincode::Decode, Debug)]
pub struct ScannedHash<T: Sized + bincode::Encode + bincode::Decode<()>> {
    pub offset: u64,
    pub hash: T,
}

pub struct ScannedArray {
    pub offset: u64,
    pub count: usize,
    pub class: u32,
}

pub fn scan_file(
    context: &ScannerContext,
    data: &[u8],
    entry: Option<&UEntryHeader>,
    mode: ScannerMode,
) -> ScanResult {
    profiling::scope!(
        "scan_file",
        format!("data len = {} bytes", data.len()).as_str()
    );

    let mut r = ScanResult::default();

    if let Some(entry) = entry
        && let Some(class) = get_class_by_id(entry.reference)
        && class.name == "s_pattern_component"
        && data.len() > 32
    {
        let ptr_offset = u64_from_endian(context.endian, data[0x10..0x18].try_into().unwrap());
        let offset = 0x10 + ptr_offset as usize;
        if offset < data.len() && offset > 0x10 {
            r.secondary_class = Some(u32_from_endian(
                context.endian,
                data[offset - 4..offset].try_into().unwrap(),
            ));
        }
    }

    // Pass 1: find array ranges we should skip (classes marked with @block_tags)
    let mut blocked_ranges = vec![];
    for offset in (0..data.len()).step_by(4) {
        if offset + 4 > data.len() {
            break;
        }
        let m: [u8; 4] = data[offset..offset + 4].try_into().unwrap();
        let value = u32_from_endian(context.endian, m);

        if matches!(
            value,
            0x80809fbd | // Pre-BL
            0x80809fb8 | // Post-BL
            0x80800184 |
            0x80800142 |
            0x8080bfcd // Marathon
        ) {
            let array_offset = offset as u64 + 4;
            let array: Option<(u64, u32)> = (|| {
                let mut c = Cursor::new(&data);
                c.seek(SeekFrom::Start(array_offset)).ok()?;
                if matches!(
                    package_manager().version,
                    GameVersion::Destiny(DestinyVersion::DestinyInternalAlpha)
                        | GameVersion::Destiny(DestinyVersion::DestinyTheTakenKing)
                ) {
                    Some((c.read_be::<u32>().ok()? as u64, c.read_be::<u32>().ok()?))
                } else {
                    Some((c.read_le::<u64>().ok()?, c.read_le::<u32>().ok()?))
                }
            })();

            if let Some((count, class)) = array {
                if let Some(class) = get_class_by_id(class) {
                    if class.block_tags {
                        let array_size = class.array_size(count as usize).unwrap_or(count as usize);
                        blocked_ranges.push(array_offset..array_offset + array_size as u64);
                    }
                }
            }
        }
    }

    // Pass 2: everything else
    for offset in (0..data.len()).step_by(4) {
        if offset + 4 > data.len() {
            break;
        }

        if blocked_ranges
            .iter()
            .any(|range| range.contains(&(offset as u64)))
        {
            continue;
        }

        let m: [u8; 4] = data[offset..offset + 4].try_into().unwrap();
        let value = u32_from_endian(context.endian, m);
        let hash = TagHash(value);

        if hash.0 != 0x811C9DC5
            && hash.is_pkg_file()
            && context.valid_file_hashes.binary_search(&hash).is_ok()
        {
            r.file_hashes.push(ScannedHash {
                offset: offset as u64,
                hash,
            });
        }

        if mode != ScannerMode::Tags {
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

        if value != 0x811c9dc5 && context.known_wordlist_hashes.binary_search(&value).is_ok() {
            r.wordlist_hashes.push(ScannedHash {
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

    if mode == ScannerMode::Hashes {
        r.file_hashes.clear();
        r.file_hashes64.clear();
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
            GameVersion::Destiny(DestinyVersion::DestinyInternalAlpha)
                | GameVersion::Destiny(DestinyVersion::DestinyTheTakenKing)
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

pub fn cache_path() -> PathBuf {
    let cache_name = format!("tags_{}.cache", package_manager().cache_key());
    exe_relative_path(&cache_name)
}

pub fn load_tag_cache() -> TagCache {
    let cache_file_path = cache_path();

    if let Ok(CacheLoadResult::Loaded(cache)) = TagCache::load(&cache_file_path) {
        return cache;
    }

    *SCANNER_PROGRESS.write() = ScanStatus::CreatingScanner;
    let scanner_context = Arc::new(
        ScannerContext::create(&package_manager()).expect("Failed to create scanner context"),
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
        .map_with(scanner_context.clone(), |context, path| {
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

            let mut all_tags: Vec<(usize, UEntryHeader)> = pkg
                .entries()
                .iter()
                .enumerate()
                .filter(|(_, e)| {
                    let tagtype = TagType::from_type_subtype_for_version(
                        version,
                        e.file_type,
                        e.file_subtype,
                    );
                    matches!(
                        tagtype,
                        TagType::Tag
                            | TagType::TagGlobal
                            | TagType::WwiseInitBank
                            | TagType::WwiseBank // WWise banks are included to allow for reverse hash lookup
                    )
                })
                .map(|(i, e)| (i, e.clone()))
                .collect();

            // Sort tags by starting block index to optimize sequential block reads
            all_tags.sort_by_key(|v| v.1.starting_block);

            let mut results = FxHashMap::default();
            for (t, e) in all_tags {
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

                let scanner_mode = match TagType::from_type_subtype_for_version(
                    version,
                    e.file_type,
                    e.file_subtype,
                ) {
                    TagType::WwiseInitBank | TagType::WwiseBank => ScannerMode::Hashes,
                    _ => ScannerMode::Both,
                };

                let mut scan_result = scan_file(context, &data, Some(&e), scanner_mode);
                if let GameVersion::Destiny(v) = version {
                    if v.is_d1() {
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
                }
                results.insert(hash, scan_result);
            }

            results
        })
        .flatten()
        .collect();

    let mut cache = transform_tag_cache(cache);
    cache.wordlist_hash = scanner_context.wordlist_hash;

    *SCANNER_PROGRESS.write() = ScanStatus::WritingCache;
    info!("Compressing tag cache...");
    let mut writer = zstd::Encoder::new(File::create(cache_file_path).unwrap(), 3).unwrap();

    bincode::encode_into_std_write(&cache, &mut writer, bincode::config::standard()).unwrap();
    writer.finish().unwrap();
    *SCANNER_PROGRESS.write() = ScanStatus::None;

    cache
}

/// Transforms the tag cache to include reference lookup tables
fn transform_tag_cache(cache: FxHashMap<TagHash, ScanResult>) -> cache::TagCache {
    info!("Transforming tag cache...");

    let mut new_cache: cache::TagCache = Default::default();

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
            if let Some(t32) = package_manager().lookup.tag64_entries.get(&t64.hash.0) {
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

#[derive(PartialEq)]
pub enum ScannerMode {
    Tags,
    Hashes,
    Both,
}
