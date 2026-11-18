# Supplemental native regression candidate

The selected product is `numera-engine`, with `numera-browser-runtime` for browser
operations. See [setup](setup.md) for normal installation and use.

`numera-native-candidate` remains an explicit intermediate for the retained native
regression tests and comparison against checkpoint
`3bf1d839fd33e42a10b982bd8607a7052ea7c4cd`. It uses the same Rust application entry
and never forwards application commands to an interpreter.

```sh
cargo build --manifest-path engine-rust/Cargo.toml --locked --no-default-features --features native-candidate --bin numera-native-candidate
```

Build tooling reads Cargo's current `compiler-artifact` and `build-finished`
records and verifies compiled source identity before staging an executable.
Candidate artifacts are separate from complete product installers and portable
bundles. See [native build tooling](../tools/xtask/README.md).

Current performance comparisons use the genuine immutable native checkpoint and
the current native product with the same Rust SDK transport and resources. The
two reviewed checkpoint transport bridges have their own source and patch
receipts. Results do not claim Python performance or rollback compatibility.

[Rewrite verification](rust-rewrite.md) describes the required functional,
security, ownership and platform gates. The full historical candidate narrative
is retained in [the reference checkpoint](https://github.com/KiaroSama/Numera-Browser/blob/3bf1d839fd33e42a10b982bd8607a7052ea7c4cd/docs/migration-candidate.md).
