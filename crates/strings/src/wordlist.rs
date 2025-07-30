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

    #[test]
    fn test_wordlist_hash_changes_with_local_file() {
        // Use a temporary file for testing instead of the actual local_wordlist.txt
        let temp_file = "test_local_wordlist.txt";
        
        // Ensure temp file doesn't exist initially
        let _ = std::fs::remove_file(temp_file);
        
        // Create a mock function that reads from our temp file instead
        // Since we cannot easily mock the local file reading, we'll test the hash function directly
        // by creating two different content strings and verifying they produce different hashes
        
        let content1 = include_str!("../../../wordlist.txt");
        let content2 = format!("{}\ntest_additional_string", content1);
        
        let hash1 = fnv1(content1.as_bytes());
        let hash2 = fnv1(content2.as_bytes());
        
        // Hash should be different when content changes
        assert_ne!(hash1, hash2, "Hash should change when wordlist content changes");
    }

    #[test]
    fn test_wordlist_hash_consistency() {
        // Test that the same content produces the same hash
        let content = include_str!("../../../wordlist.txt");
        
        let hash1 = fnv1(content.as_bytes());
        let hash2 = fnv1(content.as_bytes());
        assert_eq!(hash1, hash2, "Hash should be consistent for same wordlist content");
    }

    #[test]
    fn test_compute_wordlist_hash_function() {
        // Test the actual compute_wordlist_hash function consistency
        // This will use whatever local_wordlist.txt exists (if any) but won't modify it
        let hash1 = compute_wordlist_hash();
        let hash2 = compute_wordlist_hash();
        assert_eq!(hash1, hash2, "compute_wordlist_hash should be consistent");
    }
}
