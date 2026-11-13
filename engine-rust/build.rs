//! Embed the same product/source identity in Windows version resources as in
//! the native --build-info receipt. No signing key or certificate is read here.
fn main() {
    println!("cargo:rerun-if-env-changed=GITHUB_SHA");
    println!("cargo:rerun-if-env-changed=GITHUB_RUN_ID");
    #[cfg(windows)]
    windows_resources();
}

#[cfg(windows)]
fn windows_resources() {
    use winresource::{VersionInfo, WindowsResource};
    let version = env!("CARGO_PKG_VERSION");
    let parts: Vec<u16> = version
        .split('.')
        .map(|part| {
            part.parse()
                .expect("Product version needs numeric Windows components")
        })
        .collect();
    assert_eq!(parts.len(), 3, "Product version must have three components");
    let number =
        (u64::from(parts[0]) << 48) | (u64::from(parts[1]) << 32) | (u64::from(parts[2]) << 16);
    let source = std::env::var("GITHUB_SHA").unwrap_or_default();
    let generation = std::env::var("GITHUB_RUN_ID").unwrap_or_default();
    for (feature, binaries) in [
        (
            "CARGO_FEATURE_NATIVE_PRODUCT",
            &["numera-engine", "numera-http-observer"][..],
        ),
        (
            "CARGO_FEATURE_NATIVE_BROWSER",
            &["numera-browser-runtime"][..],
        ),
        (
            "CARGO_FEATURE_NATIVE_CANDIDATE",
            &["numera-native-candidate"][..],
        ),
    ] {
        if std::env::var_os(feature).is_none() {
            continue;
        }
        for binary in binaries {
            let mut resource = WindowsResource::new();
            resource
                .set_language(0x0409)
                .set("CompanyName", "KiaroSama")
                .set("ProductName", "Numera Browser")
                .set("FileDescription", "Numera Browser native application")
                .set("FileVersion", version)
                .set("ProductVersion", version)
                .set("InternalName", binary)
                .set("OriginalFilename", &format!("{binary}.exe"))
                .set("NumeraSourceSHA", &source)
                .set("NumeraGeneration", &generation)
                .set_version_info(VersionInfo::FILEVERSION, number)
                .set_version_info(VersionInfo::PRODUCTVERSION, number)
                .set_version_info(VersionInfo::FILETYPE, 1);
            resource
                .compile_for(binary)
                .expect("Windows product resource compilation failed");
        }
    }
}
