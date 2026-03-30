use std::{
    hash::{DefaultHasher, Hasher},
    sync::Arc,
};

use binrw::Endian;
use itertools::Itertools;
use log::info;
use quicktag_strings::{
    localized::{StringCache, create_stringmap},
    wordlist::load_wordlist,
};
use rustc_hash::FxHashMap;
use tiger_pkg::{PackageManager, TagHash, TagHash64, Version};

use crate::signatures::{SIGNATURE_LIST, SIGNATURES_HASH, Signature};

// Shareable read-only context
pub struct ScannerContext {
    pub valid_file_hashes: Vec<TagHash>,
    pub valid_file_hashes64: Vec<TagHash64>,
    pub known_string_hashes: Vec<u32>,
    pub known_wordlist_hashes: Vec<u32>,
    pub wordlist_hash: u64,
    pub signatures: Arc<FxHashMap<Signature, String>>,
    pub signatures_hash: u64,
    pub endian: Endian,
}

impl ScannerContext {
    pub fn create(package_manager: &PackageManager) -> anyhow::Result<Self> {
        info!("Creating scanner context");

        // TODO(cohae): TTK PS4 is little endian
        let endian = package_manager.version.endian();

        let stringmap = create_stringmap()?;
        crate::signatures::load_sigfile();

        let mut wordlist_hasher = DefaultHasher::new();
        let mut wordlist = StringCache::default();
        load_wordlist(|s, h| {
            wordlist_hasher.write(s.as_bytes());
            let entry = wordlist.entry(h).or_default();
            if entry.iter().any(|s2| s2 == s) {
                return;
            }

            entry.push(s.to_string());
        });

        let mut res = Self {
            valid_file_hashes: package_manager
                .lookup
                .tag32_entries_by_pkg
                .iter()
                .flat_map(|(pkg_id, entries)| -> _ {
                    entries
                        .iter()
                        .enumerate()
                        .map(|(entry_id, _)| TagHash::new(*pkg_id, entry_id as _))
                        .collect_vec()
                })
                .collect(),
            valid_file_hashes64: package_manager
                .lookup
                .tag64_entries
                .keys()
                .map(|&v| TagHash64(v))
                .collect(),
            known_string_hashes: stringmap.keys().cloned().collect(),
            known_wordlist_hashes: wordlist.keys().cloned().collect(),
            wordlist_hash: wordlist_hasher.finish(),
            signatures: SIGNATURE_LIST.load_full(),
            signatures_hash: SIGNATURES_HASH.load(std::sync::atomic::Ordering::Relaxed),
            endian,
        };

        res.valid_file_hashes.sort_unstable();
        res.valid_file_hashes64.sort_unstable();
        res.known_string_hashes.sort_unstable();
        res.known_wordlist_hashes.sort_unstable();

        Ok(res)
    }
}
