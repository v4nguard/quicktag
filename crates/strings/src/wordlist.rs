use std::{io::BufRead, time::Instant};

use log::{error, info};
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
    let file = match std::fs::File::open("local_wordlist.txt") {
        Ok(f) => f,
        Err(e) => {
            error!("Failed to load local wordlist: {}", e);
            return;
        }
    };

    let reader = std::io::BufReader::with_capacity(1024 * 1024 * 4, file);
    let mut line_count = 0;
    for line in reader.lines() {
        let Ok(line) = line else {
            break;
        };

        let h = fnv1(line.as_bytes());
        // Skip empty strings
        if h == FNV1_BASE {
            continue;
        }
        callback(&line, h);
        line_count += 1;
    }

    info!(
        "Loaded {} strings from on-disk wordlist in {}ms",
        line_count,
        load_start.elapsed().as_millis()
    );
}

/// Computes a combined hash of the embedded wordlist and local wordlist
/// This hash can be used to detect when wordlists have changed and cache needs regeneration
pub fn compute_wordlist_hash() -> u32 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    use std::hash::{Hash, Hasher};
    
    // Hash the embedded wordlist content
    WORDLIST.hash(&mut hasher);
    
    // Hash local wordlist if it exists
    if let Ok(local_content) = std::fs::read_to_string("local_wordlist.txt") {
        local_content.hash(&mut hasher);
    }
    
    hasher.finish() as u32
}
