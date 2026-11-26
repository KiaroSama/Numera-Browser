# Pinned fingerprint model

`model-2-2026.zip` is the unmodified February 2026 fpgen model used by Camoufox.
SHA-256: `6530b8322cdaa4ec042921c8d9a0369a0e6e0269ba636c01a7203e4a2f109936`.

The model is embedded application data. Runtime preparation performs no model
download and does not depend on test fixtures being installed. The Rust loader
also verifies all compressed members and decoded data before using their values.

Model and algorithm source: <https://github.com/scrapfly/fingerprint-generator>.
The native algorithm follows commit `fc7e489687821f691bae9308d73e294497d015fb`;
its Apache-2.0 license is retained here and in the distribution's browser notices.
Frozen source posterior observations remain under `tests/fixtures`.
