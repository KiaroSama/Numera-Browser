use super::*;
use numera_core::{
    browser::{adapter, hints, options},
    models::Profile,
    page_runtime::{self, Program},
};
use serde_json::json;
use std::{fs, io, path::Path};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repository root")
}
pub(super) fn artifact() -> Result<()> {
    source_remapping()?;
    compiler_artifact::check_generated(root())?;
    let artifact = page_runtime::artifact().as_bytes();
    let receipt: Value = serde_json::from_slice(&fs::read(
        root().join("browser-page-rust/generated/receipt.json"),
    )?)?;
    compiler_artifact::validate(root(), artifact, &receipt)?;
    let mut changed = artifact.to_vec();
    changed[0] ^= 1;
    assert!(compiler_artifact::validate(root(), &changed, &receipt).is_err());
    assert!(compiler_artifact::validate(root(), b"", &receipt).is_err());
    assert!(
        compiler_artifact::validate(
            root(),
            &vec![b'x'; compiler_artifact::MAX_ARTIFACT + 1],
            &receipt
        )
        .is_err()
    );
    for (key, value) in [
        ("schema", json!(2)),
        ("artifact", json!("unreviewed/runtime.js")),
        ("artifact_sha256", json!("0".repeat(64))),
        ("artifact_bytes", json!(artifact.len() + 1)),
        ("authored_language", json!("JavaScript")),
        ("target", json!("native")),
        ("rustc", json!("rustc 1.99.0")),
        ("wasm_bindgen", json!("wasm-bindgen 0.2.128")),
        ("binaryen", json!("wasm2js version 132")),
        ("wasm2js_binary_sha256", json!("0".repeat(64))),
        ("binaryen_archive_sha256", json!("0".repeat(64))),
        ("path_remapping", json!([])),
        ("wasm2js_flags", json!(["-O"])),
        ("runtime_compilation", json!(true)),
        ("minified", json!(false)),
        ("source_printer", json!({})),
        ("compiler_javascript_bytes", json!(0)),
        ("input_wasm_sha256", json!("invalid")),
        ("binding_wasm_sha256", json!(null)),
        ("bindgen_glue_sha256", json!("A".repeat(64))),
        ("compiler_javascript_sha256", json!("")),
        ("source_sha256", json!({})),
    ] {
        let mut changed = receipt.clone();
        changed[key] = value;
        assert!(
            compiler_artifact::validate(root(), artifact, &changed).is_err(),
            "receipt field {key}"
        );
    }
    for (key, value) in [
        ("name", json!("unreviewed")),
        ("version", json!("0.157.0")),
        ("mode", json!("semantic-minifier")),
        ("source_type", json!("mjs")),
        ("preserve_parentheses", json!(false)),
        ("regex_pattern_validation", json!(false)),
        ("semantic_transforms", json!(true)),
        ("identifier_mangling", json!(true)),
        ("ascii_only", json!(true)),
    ] {
        let mut changed = receipt.clone();
        changed["source_printer"][key] = value;
        assert!(
            compiler_artifact::validate(root(), artifact, &changed).is_err(),
            "printer policy field {key}"
        );
    }
    for name in ["oxc_allocator", "oxc_codegen", "oxc_parser", "oxc_span"] {
        let mut changed = receipt.clone();
        changed["source_printer"]["crate_sha256"][name] = json!("0".repeat(64));
        assert!(compiler_artifact::validate(root(), artifact, &changed).is_err());
    }
    let mut wrong_marker = artifact.to_vec();
    wrong_marker[compiler_artifact::GENERATED_BANNER.len()] ^= 1;
    for unframed in [
        artifact[compiler_artifact::GENERATED_BANNER.len()..].to_vec(),
        wrong_marker,
        artifact[..artifact.len() - 1].to_vec(),
    ] {
        let mut matching_hash = receipt.clone();
        matching_hash["artifact_sha256"] = compiler_artifact::digest(&unframed).into();
        matching_hash["artifact_bytes"] = json!(unframed.len());
        assert!(compiler_artifact::validate(root(), &unframed, &matching_hash).is_err());
    }
    for key in receipt.as_object().ok_or("receipt object")?.keys() {
        let mut missing = receipt.clone();
        missing.as_object_mut().ok_or("receipt object")?.remove(key);
        assert!(
            compiler_artifact::validate(root(), artifact, &missing).is_err(),
            "missing receipt field {key}"
        );
    }
    let mut extra = receipt.clone();
    extra["unreviewed"] = true.into();
    assert!(compiler_artifact::validate(root(), artifact, &extra).is_err());
    let owned = common::TempDirectory::new("page-artifact-contract")?;
    for relative in receipt["source_sha256"]
        .as_object()
        .ok_or("source map")?
        .keys()
    {
        let path = owned.path().join(relative);
        fs::create_dir_all(path.parent().ok_or("source parent")?)?;
        fs::copy(root().join(relative), path)?;
    }
    let generated = owned.path().join("browser-page-rust/generated");
    fs::create_dir_all(&generated)?;
    fs::write(generated.join("runtime.js"), artifact)?;
    fs::write(
        generated.join("receipt.json"),
        serde_json::to_vec(&receipt)?,
    )?;
    compiler_artifact::check_generated(owned.path())?;
    for relative in ["webrtc_rules.rs", "printer.rs"] {
        let source = owned.path().join("browser-page-rust/src").join(relative);
        let original = fs::read(&source)?;
        fs::write(
            &source,
            [original.as_slice(), b"\n// changed source\n"].concat(),
        )?;
        assert!(compiler_artifact::check_generated(owned.path()).is_err());
        fs::remove_file(&source)?;
        assert!(compiler_artifact::check_generated(owned.path()).is_err());
        fs::write(&source, &original)?;
    }
    let extra = owned.path().join("browser-page-rust/src/unrecorded.rs");
    fs::write(&extra, b"// unrecorded source\n")?;
    assert!(compiler_artifact::check_generated(owned.path()).is_err());
    fs::remove_file(extra)?;
    fs::write(generated.join("runtime.js"), changed)?;
    assert!(compiler_artifact::check_generated(owned.path()).is_err());
    fs::write(generated.join("runtime.js"), artifact)?;
    fs::write(generated.join("receipt.json"), b"{}")?;
    assert!(compiler_artifact::check_generated(owned.path()).is_err());
    owned.cleanup()?;
    Ok(())
}
fn source_remapping() -> Result<()> {
    let owned = common::TempDirectory::new("page-source-remapping")?;
    let metadata = |root: &Path| -> Result<Value> {
        let directory = root.join("registry/src/cache-layout/fixture-crate-1.2.3");
        fs::create_dir_all(&directory)?;
        let manifest = directory.join("Cargo.toml");
        fs::write(
            &manifest,
            b"[package]\nname='fixture-crate'\nversion='1.2.3'\n",
        )?;
        Ok(
            json!({"version":1,"packages":[{"source":"registry+https://example.invalid/index","name":"fixture-crate","version":"1.2.3","manifest_path":manifest}]}),
        )
    };
    let first = metadata(&owned.path().join("canonical-home"))?;
    let second = metadata(&owned.path().join("alternate-home"))?;
    let first_flags = source_paths::dependency_remaps(&first)?;
    let second_flags = source_paths::dependency_remaps(&second)?;
    assert_ne!(first_flags, second_flags);
    for flags in [&first_flags, &second_flags] {
        assert!(!flags.is_empty());
        assert!(
            flags
                .iter()
                .all(|flag| flag.ends_with("=crates/fixture-crate-1.2.3/"))
        );
    }
    let mut duplicate = first.clone();
    duplicate["packages"]
        .as_array_mut()
        .ok_or("packages")?
        .push(second["packages"][0].clone());
    assert!(source_paths::dependency_remaps(&duplicate).is_err());
    for invalid in [
        Value::Null,
        json!({"version":2,"packages":[]}),
        json!({"version":1,"packages":[]}),
        json!({"version":1,"packages":[{"source":null}]}),
    ] {
        assert!(source_paths::dependency_remaps(&invalid).is_err());
    }
    for (field, value) in [
        ("name", json!("../escape")),
        ("version", json!("bad/version")),
        ("source", json!("unknown-source")),
        ("manifest_path", json!("relative/Cargo.toml")),
    ] {
        let mut invalid = first.clone();
        invalid["packages"][0][field] = value;
        assert!(source_paths::dependency_remaps(&invalid).is_err());
    }
    let mut many = first.clone();
    many["packages"] = json!(vec![first["packages"][0].clone(); 513]);
    assert!(source_paths::dependency_remaps(&many).is_err());
    owned.cleanup()?;
    Ok(())
}
fn frame(value: &Value) -> Result<()> {
    let mut bytes = serde_json::to_vec(value)?;
    bytes.push(b'\n');
    assert!(
        bytes.len() <= 1_048_576,
        "Complete SDK frame exceeds unchanged budget: {}",
        bytes.len()
    );
    assert_eq!(
        adapter::frame(&mut io::Cursor::new(&bytes))?,
        Some(value.clone())
    );
    Ok(())
}
pub(super) fn frames(fp: &Fingerprint, script: &str) -> Result<()> {
    let profile = Profile::from_record(
        &json!({"id":"owned-frame-profile","name":"Frame budget","fingerprint":fp,"created_at":0.0,"updated_at":0.0}),
    )?;
    for backend in ["playwright", "patchright", "camoufox", "cloak"] {
        let chromium = matches!(backend, "playwright" | "patchright");
        let options = options::build(
            &profile,
            backend,
            "/owned/frame-budget/user-data/profile",
            false,
            &[],
            &[],
            None,
        )?;
        let scripts = if chromium { vec![script] } else { Vec::new() };
        let policy = if chromium {
            hints::policy(fp)?
        } else {
            Value::Null
        };
        frame(
            &json!({"protocol":1,"id":u64::MAX,"operation":"open","backend":backend,"options":options,"init_scripts":scripts,"cdp_policy":policy}),
        )?;
        for program in [
            Program::Trust,
            Program::Dns,
            Program::Font,
            Program::Ip,
            Program::Tls,
            Program::WebRtc,
            Program::LocationHost,
            Program::InternalLinks,
            Program::InspectIdentity,
            Program::HostEnvironment,
            Program::SmokeValue,
        ] {
            let argument = match program {
                Program::Trust => {
                    json!({"knownFont":fp.fonts.first(),"alienFont":"not-a-real-font-7391"})
                }
                Program::Dns => {
                    json!({"idUrl":"https://bash.ws/id","subTmpl":"https://{i}.{id}.bash.ws/","resultTmpl":"https://bash.ws/dnsleak/test/{id}?json","n":8})
                }
                Program::Font => json!(fp.fonts),
                Program::Ip => json!([
                    "https://ipapi.co/json/",
                    "https://ipwho.is/",
                    "https://api.ipify.org/?format=json"
                ]),
                _ => Value::Null,
            };
            frame(
                &json!({"protocol":1,"id":u64::MAX,"operation":"evaluate","expression":page_runtime::expression(program),"argument":argument,"tolerate_failure":true}),
            )?;
        }
    }
    let oversized = json!({"protocol":1,"id":u64::MAX,"operation":"evaluate","expression":script,"argument":"x".repeat(1_048_576),"tolerate_failure":true});
    let mut bytes = serde_json::to_vec(&oversized)?;
    bytes.push(b'\n');
    assert!(adapter::frame(&mut io::Cursor::new(bytes)).is_err());
    Ok(())
}
