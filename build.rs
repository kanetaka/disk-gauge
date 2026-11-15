fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/icon/icon.ico");

    // Windows: 実行ファイル（DiskGauge.exe）にアイコンを埋め込む。
    // エクスプローラーやショートカットに表示されるのはこのアイコン。
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon/icon.ico");
        res.set("ProductName", "DiskGauge");
        res.set("FileDescription", "DiskGauge - Disk usage visualizer");
        if let Err(e) = res.compile() {
            println!("cargo:warning=failed to embed the Windows icon resource: {e}");
        }
    }
}
