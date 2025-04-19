use binrw::Endian;
use itertools::Itertools;
use log::info;
use quicktag_strings::{
    localized::{StringCache, create_stringmap},
    wordlist::load_wordlist,
};
use tiger_pkg::{PackageManager, TagHash, TagHash64, Version};

// Shareable read-only context
pub struct ScannerContext {
    pub valid_file_hashes: Vec<TagHash>,
    pub valid_file_hashes64: Vec<TagHash64>,
    pub known_string_hashes: Vec<u32>,
    pub known_wordlist_hashes: Vec<u32>,
    pub endian: Endian,
}

impl ScannerContext {
    pub fn create(package_manager: &PackageManager) -> anyhow::Result<Self> {
        info!("Creating scanner context");

        // TODO(cohae): TTK PS4 is little endian
        let endian = package_manager.version.endian();

        let stringmap = create_stringmap()?;

        let mut wordlist = StringCache::default();
        load_wordlist(|s, h| {
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
            endian,
        };

        res.valid_file_hashes.sort_unstable();
        res.valid_file_hashes64.sort_unstable();
        res.known_string_hashes.sort_unstable();
        res.known_wordlist_hashes.sort_unstable();

        Ok(res)
    }
}
