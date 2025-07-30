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
    let mut combined_content = String::new();
    
    // Add embedded wordlist content
    combined_content.push_str(WORDLIST);
    
    // Add local wordlist content if it exists
    if let Ok(local_content) = std::fs::read_to_string("local_wordlist.txt") {
        combined_content.push_str(&local_content);
    }
    
    // Use FNV1 hash for consistency with the rest of the codebase
    fnv1(combined_content.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;

    #[test]
    fn test_wordlist_hash_changes_with_local_file() {
        // Clean up any existing local wordlist first
        let _ = std::fs::remove_file("local_wordlist.txt");
        
        // Get initial hash (embedded wordlist only)
        let initial_hash = compute_wordlist_hash();
        
        // Create local wordlist
        {
            let mut file = File::create("local_wordlist.txt").unwrap();
            writeln!(file, "test_string").unwrap();
        }
        
        let new_hash = compute_wordlist_hash();
        
        // Cleanup before assertion to avoid affecting other tests
        std::fs::remove_file("local_wordlist.txt").unwrap();
        
        // Hash should be different when local wordlist exists
        assert_ne!(initial_hash, new_hash, "Hash should change when local wordlist is present");
    }

    #[test]
    fn test_wordlist_hash_consistency() {
        // Make sure no local wordlist exists before testing
        let _ = std::fs::remove_file("local_wordlist.txt");
        
        // Hash should be consistent for same content
        let hash1 = compute_wordlist_hash();
        let hash2 = compute_wordlist_hash();
        assert_eq!(hash1, hash2, "Hash should be consistent for same wordlist content");
    }
}
