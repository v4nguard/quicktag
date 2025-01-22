use destiny_pkg::{PackageManager, TagHash, TagHash64};
use eframe::epaint::mutex::RwLock;
use lazy_static::lazy_static;
use rustc_hash::FxHashMap;
use std::sync::Arc;

lazy_static! {
    static ref PACKAGE_MANAGER: RwLock<Option<Arc<PackageManager>>> = RwLock::new(None);
    static ref PM_HASH64_LOOKUP: RwLock<FxHashMap<TagHash, TagHash64>> =
        RwLock::new(FxHashMap::default());
}

pub fn initialize_package_manager(pm: PackageManager) {
    *PACKAGE_MANAGER.write() = Some(Arc::new(pm));

    let mut hash64_lookup = PM_HASH64_LOOKUP.write();
    hash64_lookup.clear();
    for (hash64, hash) in package_manager().lookup.tag64_entries.iter() {
        hash64_lookup.insert(hash.hash32, TagHash64(*hash64));
    }
}

pub fn get_hash64(tag: TagHash) -> Option<TagHash64> {
    PM_HASH64_LOOKUP.read().get(&tag).cloned()
}

pub fn package_manager_checked() -> anyhow::Result<Arc<PackageManager>> {
    PACKAGE_MANAGER
        .read()
        .as_ref()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("Package manager is not initialized!"))
}

pub fn package_manager() -> Arc<PackageManager> {
    package_manager_checked().unwrap()
}
