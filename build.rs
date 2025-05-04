fn main() {
    println!("cargo:rerun-if-changed=assets/FileOrganizer.ico");
    println!("cargo:rerun-if-changed=Cargo.toml");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let version = std::env::var("CARGO_PKG_VERSION").expect("Package version");
        winresource::WindowsResource::new()
            .set_icon("assets/FileOrganizer.ico")
            .set_language(0x0409)
            .set("ProductName", "FileOrganizer")
            .set(
                "FileDescription",
                "FileOrganizer - AI-assisted file organization",
            )
            .set("InternalName", "FileOrganizer")
            .set("OriginalFilename", &format!("FileOrganizer-v{version}.exe"))
            .compile()
            .expect(
                "Could not embed the application icon and version; Windows SDK rc.exe is required",
            );
    }
}
