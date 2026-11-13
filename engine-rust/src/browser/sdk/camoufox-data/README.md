# Camoufox identity catalogs

The font, media and voice catalogs are copied without changes from
`python/src/camoufox` at `daijro/camoufox` commit
`079a6ce8249bab98063e5ac9241e3879a3e6578d`. `launcher-constants.json` is
from `df9ca1156abf94bf5e8e7764effd35d3a7b298f9`, which adds the CPU tables;
the existing font, media and voice inputs remain unchanged.
They retain the upstream Mozilla Public License 2.0:
https://github.com/daijro/camoufox/blob/079a6ce8249bab98063e5ac9241e3879a3e6578d/LICENSE

The native port is in `../camoufox_identity.rs`. Per-file SHA-256 digests and
frozen original draws are recorded in `tests/fixtures/camoufox-identity.json`
relative to the `engine-rust` crate. The same reported browser-policy test checks
all 240 font inputs, 30 media-device inputs and 90 speech-voice inputs, as well
as the SHA-256 of all eight catalogs; existing test identities remain.

CPU preparation follows the newer source, but CPU computation emulation requires
both active `cpu:arch` and `cpu:manufacturer` declarations in the selected
package. The pinned official `158.0.1-beta.41` package has neither: it receives
no new CPU identity fields or CPU emulation preferences. A request that requires
them is rejected on that package. No unreleased canvas, audio, Opus, wasm or
NaN computation patch is claimed. Source/version boundaries and validation are
recorded in `docs/camoufox-upstream-compatibility.json`.
