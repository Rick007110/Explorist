//! Embeds the app icon and version details in explorist.exe.

fn main() {
    println!("cargo:rerun-if-changed=assets/icon/icon.ico");
    // Only when building on Windows for Windows: the resource compiler is
    // part of the Windows toolchain.
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon/icon.ico");
        res.set("FileDescription", "Explorist");
        res.set("ProductName", "Explorist");
        if let Err(err) = res.compile() {
            println!("cargo:warning=couldn't embed the icon: {err}");
        }
    }
}
