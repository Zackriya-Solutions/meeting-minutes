fn main() {
    #[cfg(target_os = "macos")]
    {
        println!("cargo:rustc-link-lib=framework=AVFoundation");
        println!("cargo:rustc-link-lib=framework=Cocoa");
        println!("cargo:rustc-link-lib=framework=Foundation");

        // Let the enhanced_macos crate handle its own Swift compilation
        // The swift-rs crate build will be handled in the enhanced_macos crate's build.rs
    }
    tauri_build::build()
}
