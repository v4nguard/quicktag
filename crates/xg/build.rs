fn main() {
    // Copy all DLLS from lib/ to the exe directory
    let project_path = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let exe_dir = out_dir
        .ancestors()
        .nth(3)
        .expect("Failed to get exe directory");
    let dll_path = project_path.join("xg.dll");
    println!("Copying {} to {}", dll_path.display(), exe_dir.display());

    std::fs::copy(dll_path, exe_dir.join("xg.dll")).unwrap();
}
