//! Embeds the icon and version metadata into the Windows executable.

fn main() {
    println!("cargo:rerun-if-changed=../../assets/cueline.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../assets/cueline.ico")
            .set("FileDescription", "CueLine — LTC & MTC timeline player")
            .set("ProductName", "CueLine")
            .set("LegalCopyright", "MIT License, CueLine contributors");
        if let Err(e) = res.compile() {
            // Missing Windows SDK: build without an embedded icon.
            println!("cargo:warning=could not embed icon: {e}");
        }
    }
}
