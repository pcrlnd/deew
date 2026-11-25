//! The frontend (`../dist`) is embedded into the binary, so it has to be built first.

fn main() {
    println!("cargo:rerun-if-changed=../dist");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../src-tauri/icons/icon.ico");
    let index = std::path::Path::new("../dist/index.html");
    if !index.is_file() {
        println!(
            "cargo:warning=frontend not built: ../dist/index.html is missing. Run `npm run build` in gui/ first."
        );
        panic!(
            "frontend not built: gui/dist/index.html is missing. Run `npm run build` in gui/ and build deew-web again."
        );
    }

    // Embed the application icon into the Windows executable (same icon as deew-gui).
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if target_os == "windows" && target_env == "msvc" {
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon("../src-tauri/icons/icon.ico");
        if let Err(e) = resource.compile() {
            println!("cargo:warning=couldn't embed ../src-tauri/icons/icon.ico: {e}");
            std::process::exit(1);
        }
    }
}
