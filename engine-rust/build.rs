//! Embed the same product/source identity in Windows version resources as in
//! the native --build-info receipt. No signing key or certificate is read here.
#[path = "build_support/openapi.rs"]
mod openapi;

fn main() {
    println!("cargo:rerun-if-env-changed=GITHUB_SHA");
    println!("cargo:rerun-if-env-changed=GITHUB_RUN_ID");
    openapi_schema();
    #[cfg(windows)]
    windows_resources();
}

fn openapi_schema() {
    use std::{env, fs, path::PathBuf};

    println!("cargo:rerun-if-changed=src/http/documentation-assets/openapi.json");
    println!("cargo:rerun-if-changed=src/http/documentation-assets/openapi-native.json");
    println!("cargo:rerun-if-changed=build_support/openapi.rs");
    let schema: serde_json::Value =
        serde_json::from_str(include_str!("src/http/documentation-assets/openapi.json"))
            .expect("Embedded HTTP documentation schema is invalid");
    let additions: serde_json::Value = serde_json::from_str(include_str!(
        "src/http/documentation-assets/openapi-native.json"
    ))
    .expect("Native HTTP documentation additions are invalid");
    let mut schema = openapi::extend(schema, &additions)
        .expect("Native HTTP documentation must preserve the historical contract");
    schema["info"]["version"] = env!("CARGO_PKG_VERSION").into();
    let path = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo output directory missing"));
    let schema =
        serde_json::to_vec(&schema).expect("HTTP documentation schema serialization failed");
    fs::write(path.join("openapi.json"), schema).expect("HTTP documentation schema write failed");
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
                .set_icon("../desktop/src-tauri/icons/icon.ico")
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
