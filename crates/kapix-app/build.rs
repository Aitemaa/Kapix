fn main() {
    // Embed a PerMonitorV2 DPI-aware, Common-Controls-v6, asInvoker manifest (pure Rust, no rc.exe).
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        use embed_manifest::manifest::DpiAwareness;
        embed_manifest::embed_manifest(
            embed_manifest::new_manifest("Kapix").dpi_awareness(DpiAwareness::PerMonitorV2),
        )
        .expect("unable to embed manifest");
    }
    println!("cargo:rerun-if-changed=build.rs");
}
