use std::{io::BufRead, time::Instant};

use lazy_static::lazy_static;
use log::{error, info};
use quicktag_core::util::{FNV1_BASE, fnv1};

lazy_static! {
    static ref WORDLIST: String = {
        info!("Decompressing wordlist...");
        const DATA: &[u8] = include_bytes!("../wordlist.zst");
        let decompressed = zstd::stream::decode_all(&mut std::io::Cursor::new(DATA)).unwrap();
        String::from_utf8(decompressed).unwrap()
    };
}

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
