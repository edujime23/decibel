use std::env;
use std::path::PathBuf;

fn main() {
    let include_path = PathBuf::from("../steamaudio/include");

    // Re-run this script if any header files change
    println!("cargo:rerun-if-changed={}", include_path.display());

    // Link against the phonon library for compilation based on target OS
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_else(|_| "windows".to_string());
    let lib_path = match target_os.as_str() {
        "windows" => PathBuf::from("../steamaudio/windows/x64"),
        "linux" => PathBuf::from("../steamaudio/linux/x64"),
        "macos" => PathBuf::from("../steamaudio/macos"),
        _ => PathBuf::from("../steamaudio/windows/x64"),
    };

    println!("cargo:rustc-link-search=native={}", lib_path.display());
    println!("cargo:rustc-link-lib=phonon");

    // Generate bindings dynamically from phonon.h
    let bindings = bindgen::Builder::default()
        .header(include_path.join("phonon.h").to_str().unwrap())
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .expect("Unable to generate Steam Audio bindings");

    // Write the bindings to the output directory
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}