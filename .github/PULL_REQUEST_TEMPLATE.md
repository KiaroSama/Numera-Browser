## What & why

Describe the change and the reason for it.

## Checklist

- [ ] Relevant Rust tests, formatting and strict Clippy pass
- [ ] Source test identities, kinds and platform conditions remain intact (`cargo xtask ci coverage`)
- [ ] Fingerprint validation passes if identity preparation changed
- [ ] Rust dashboard builds if UI changed (`cargo run --manifest-path dashboard/Cargo.toml --locked --bin dashboard-build`)
- [ ] Generated browser runtime and its receipt are current if browser-page Rust changed
- [ ] Docs updated if needed
