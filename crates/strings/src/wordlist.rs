use std::time::Instant;

use log::info;
use quicktag_core::util::{FNV1_BASE, fnv1};

const WORDLIST: &str = include_str!("../../../wordlist.txt");

pub fn load_wordlist<F: FnMut(&str, u32)>(mut callback: F) {
    let load_start = Instant::now();
    for s in WORDLIST.lines() {
        let s = s.to_string();
        let h = fnv1(s.as_bytes());
        // Skip empty strings
        if h == FNV1_BASE {
            continue;
        }
        callback(&s, h);
    }
    info!(
        "Loaded {} strings from embedded wordlist in {}ms",
        WORDLIST.lines().count(),
        load_start.elapsed().as_millis()
    );

    let load_start = Instant::now();
    let Ok(wordlist_disk) = std::fs::read_to_string("local_wordlist.txt") else {
        return;
    };

    for s in wordlist_disk.lines() {
        let s = s.to_string();
        let h = fnv1(s.as_bytes());
        // Skip empty strings
        if h == FNV1_BASE {
            continue;
        }
        callback(&s, h);
    }

    info!(
        "Loaded {} strings from on-disk wordlist in {}ms",
        wordlist_disk.lines().count(),
        load_start.elapsed().as_millis()
    );
}
