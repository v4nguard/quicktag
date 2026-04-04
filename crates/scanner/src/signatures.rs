use anyhow::Context;
use arc_swap::ArcSwap;
use itertools::Itertools;
use log::{error, info};
use rustc_hash::FxHashMap;
use std::{
    hash::{DefaultHasher, Hasher},
    path::Display,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

#[derive(Debug, Hash, Eq, PartialEq, Clone, bincode::Encode, bincode::Decode)]
pub enum Signature {
    U32(u32),
    U64(u64),
}

lazy_static::lazy_static! {
    pub static ref SIGNATURE_LIST: ArcSwap<FxHashMap<Signature, String>> = ArcSwap::new(Default::default());
    pub static ref SIGNATURES_HASH: AtomicU64 = AtomicU64::new(0);
}

pub fn load_sigfile() {
    let Ok(sigfile) = std::fs::read_to_string("signatures.csv") else {
        return;
    };

    match parse_sigfile(&sigfile) {
        Ok(o) => {
            info!("Loaded {} signatures", o.len());
            SIGNATURE_LIST.store(Arc::new(o));
        }
        Err(e) => {
            error!("Failed to parse signatures file: {:?}", e);
        }
    }
}

fn parse_sigfile(s: &str) -> anyhow::Result<FxHashMap<Signature, String>> {
    let mut hasher = DefaultHasher::new();
    hasher.write(s.as_bytes());
    SIGNATURES_HASH.store(hasher.finish(), Ordering::Relaxed);

    let mut signatures: FxHashMap<Signature, String> = Default::default();

    // signatures.csv lines are formatted as 64-bit or 32-bit hex integers
    // 0x11223344AABBCCDD,Some neat name for your signature (Anything Unicode goes 🙂)
    // 0x1122AABB,Vigilance Wing
    for l in s.lines() {
        if l.trim().is_empty() || l.starts_with("#") {
            continue;
        }
        let mut parts = l.split(',');
        let sig = parts.next().context("Missing signature")?;
        let name = parts.join(",");

        let signature = if let Ok(int) = u32::from_str_radix(sig.trim_start_matches("0x"), 16) {
            Signature::U32(int)
        } else {
            let int = u64::from_str_radix(sig.trim_start_matches("0x"), 16)
                .context("Invalid 64-bit hex key")?;
            Signature::U64(int)
        };

        signatures.insert(signature, name);
    }

    Ok(signatures)
}
