use std::{fs::File, io::BufReader};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../wordlist.txt");

    let source =
        BufReader::new(File::open("../../wordlist.txt").expect("Failed to open wordlist.txt"));
    let destination = File::create("wordlist.zst").expect("Failed to create wordlist.zst");
    zstd::stream::copy_encode(source, destination, 11).expect("Failed to compress wordlist.txt");
}
