# Browser page runtime

Browser identity and diagnostic behavior is authored in Rust. `identity.rs`
owns identity preparation; `probes.rs` owns DOM measurements, browser fetches,
TLS response interpretation and bounded WebRTC candidate collection.

The browser API exports synchronous `apply_identity(config)` and Promise-based
`trust_probe(config)`, `dns_probe(config)`, `font_probe(fonts)`, `ip_probe(urls)`,
`tls_probe()` and `webrtc_probe()`. Their return values retain the original
probe objects, null observations and failure boundaries. WebRTC closes its
connection on completion or after three seconds, then releases its handlers.
`location_host()`, `internal_links(host)`, `inspect_identity()`,
`host_environment()` and `smoke_value()` also keep browser-side warmup,
inspection, Camoufox model conditioning and image acceptance logic in Rust.

The native controller loads a compiler-produced artifact. Ordinary WASM
compilation is blocked by strict page CSP, including the tested DevTools
evaluation path. The distribution builder therefore translates the same WASM
to generated JavaScript; its receipt must identify the Rust sources and tools.
Generated output is not an alternative authored implementation. The controller
keeps its existing 1 MiB serialized SDK request limit for initialization and
probe requests.

The compiler remaps each locked dependency's actual source directory to its
crate name and version. Registry cache hashes and mirror directory names must
not enter the shipped program through panic locations. Verification compares
the full artifact and receipt bytes; CI preserves compiler intermediates and
their hashes when that comparison fails.

Development parity compares the Rust exports with the original probes in a real
browser. It does not add source test identities or substitute static expected
objects for browser observations.

Regenerate from the repository root with Rust 1.99.1, the
`wasm32-unknown-unknown` target, wasm-bindgen CLI 0.2.129 and the checksum-pinned
Linux x64 Binaryen 133 `wasm2js` executable:

```sh
cargo run --manifest-path browser-page-rust/Cargo.toml --locked --bin browser-page-build -- --wasm2js /path/to/wasm2js
cargo run --manifest-path browser-page-rust/Cargo.toml --locked --bin browser-page-build -- --wasm2js /path/to/wasm2js --check
```

The checked-in `generated/runtime.js` is generated output, with its Rust inputs,
tool hashes and target recorded in `generated/receipt.json`. CI requires a fresh
deterministic rebuild and checks the actual WASM and native build-tool targets.
The native engine embeds the generated factory so an ordinary application build
needs neither Binaryen nor a runtime download. Editing Rust sources requires
regenerating the artifact and receipt before their freshness checks can pass.
