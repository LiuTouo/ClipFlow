fn main() {
    // tauri-build embeds the Common-Controls v6 manifest (via its Windows
    // resource) only into app BIN targets. The lib's TEST harness links the
    // same dependency graph (muda/tray-icon import TaskDialogIndirect), and
    // without that manifest the loader binds comctl32 5.82, which lacks the
    // entry point — the test exe dies at load with 0xc0000139. Embed the
    // identical dependency only into the explicit library test target declared
    // in Cargo.toml. Bins already carry the manifest through tauri-build's
    // resource, and a duplicate manifest fails the link (LNK1123).
    if std::env::var("CARGO_CFG_WINDOWS").is_ok() {
        println!("cargo:rustc-link-arg-tests=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg-tests=/MANIFESTDEPENDENCY:type='win32' \
             name='Microsoft.Windows.Common-Controls' version='6.0.0.0' \
             processorArchitecture='*' publicKeyToken='6595b64144ccf1df' \
             language='*'"
        );
    }
    tauri_build::build()
}
