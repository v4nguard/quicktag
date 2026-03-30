fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../wordlist.txt");

    // let source =
    //     BufReader::new(File::open("../../wordlist.txt.zst").expect("Failed to open wordlist.txt"));
    // let destination = File::create("wordlist.zst").expect("Failed to create wordlist.zst");
    std::fs::copy("../../wordlist.txt.zst", "wordlist.zst");
    // zstd::stream::copy_encode(source, destination, 11).expect("Failed to compress wordlist.txt");
}
