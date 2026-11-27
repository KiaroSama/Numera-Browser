# Portable updates and release verification

## Status and scope

The updater handles complete, signed portable release generations. A generation contains the shell, Rust manager, browser worker, dashboard and notices. Available browser resources vary by platform. Source checkouts, containers, installer-based deployments and CLI-only archives update through their own installation routes.

The [Rust verification record](rust-rewrite.md) tracks CI acceptance. Published portable releases carry the signed manifests and artifacts described below.

## End-user flow

1. The portable desktop checks GitHub after readiness and at most once every 24 hours while open. Use **Check for updates** to check manually. No OS background service runs the updater.
2. In **Settings > Software updates**, review the signed offer, its size, the browser changes and the restart requirement. GitHub release notes alone do not authenticate an offer.
3. For a manual update, **Download update > Approve download** accepts only the selected offer. **Later** skips the download. Use **Install and restart > Approve restart** separately after verification.
4. The updater starts activation only after it can prove affected work is idle. Active sessions, jobs or uncertain ownership defer installation. The new generation must prove its identity and readiness before the app accepts work again.

When automatic app updates are enabled, the desktop can download and install a signed offer on its own. Turning off automatic browser-core updates also blocks automatic installation of an offer that would change the browser components. Signed standalone browser-core updates are not currently available.

Keep enough writable space for the archive, expanded files and rollback. Managed files changed outside the supported release process can prevent verification. Do not bypass that refusal by deleting manifests or clearing ownership records.

### Browser updates

The manager tracks the driver and browser versions bundled with each portable release. A manager update can also update these matched binaries. Explicit external browser paths remain user-managed. Numera Core Browser is planned as a separate runtime, as described in the [architecture](architecture.md#product-and-runtime-boundary).

CloakBrowser's proprietary browser binary is not redistributed. First acquisition is a separate approved vendor operation. The wrapper's background mutation is suppressed; missing resources do not grant implicit download consent. Free-key/latest-only or otherwise unsupported vendor version contracts cannot be advertised as an exact tested pin. Existing permitted local overrides remain user-managed.

## What is signed

Two trust layers have distinct purposes:

| Layer | What it proves | What it does not prove |
|---|---|---|
| Ed25519 release/BUILD signatures | Exact metadata bytes were signed by the pinned project key; hashes bind owned files and archives. | Public OS publisher trust, notarization, absence of bugs. |
| Windows Authenticode | The MSI or executable matches the designated publisher certificate. | Public trust for that self-signed certificate or automatic SmartScreen acceptance. |

The public manifest verification key is [release-signing-key.pem](release-signing-key.pem).
The native private-trust certificate is [code-signing-cert.pem](code-signing-cert.pem).
These are **public** verification material, not private keys. Obtain them from a trusted project checkout/distribution; a key downloaded from an untrusted mirror alongside an archive is not a trust anchor.

The Windows certificate is self-signed. Do not install it as a system root to suppress warnings. The detached signature check below uses a trusted certificate copy without changing the system trust store.

## Manifest protocol 1

`update-manifest.sig` is a raw **64-byte Ed25519 signature** over the exact UTF-8 bytes of `update-manifest.json`. This is not a base64 Tauri/minisign signature. Do not reformat JSON before verification.

The manifest authenticates repository/product, version, full source SHA, positive monotonic release generation, creation/expiry times, minimum protocol, per-platform SDK/browser metadata and each approved archive's size/SHA-256, expanded bounds, member limit, root and BUILD hash. The generation uses the producing GitHub run identity, not lexicographic SHA comparison. The updater considers both version tags and legacy `build-<sha>` releases. `/releases/latest` alone would miss build prereleases.

Inside a portable archive, `BUILD.json` and raw `BUILD.sig` authenticate the initial installed file inventory and identity. Regular-file records include path/size/SHA-256/executable status; permitted symbolic links carry contained destinations. `PROVENANCE.json` holds extended build/browser/license information. BUILD cannot hash itself or its signature; the outer signed archive/BUILD hashes close that cycle. After extraction and again immediately before activation, the staged BUILD version/source/generation/platform, SDK/browser components and exact inventory digest must match the authenticated offer. A second valid signature alone does not authorize substituting another signed inventory.

Discovery permits only approved HTTPS GitHub release hosts, with bounded metadata, redirects and network deadlines. A failed metadata/signature request, untrusted candidate or exhausted page bound reports an incomplete check, not an up-to-date installation; a previously displayed offer is cleared before a new check. Authenticated older, expired or incompatible generations are excluded normally. Checking retrieves metadata only, never an application archive. Downloads stream to disk; declared archive/expanded/member bounds, path/link containment and final file hashes are checked before live replacement. The pinned public key is never replaced by an unsigned feed response. An unknown protocol, wrong platform/SDK, bad signature, hash mismatch, stale/replayed generation or malformed inventory fails closed.

## Verify a downloaded release

Download the manifest, signature and **only your compatible platform archive** from the same completed release. Use an independently trusted copy of the public verification key. With a current OpenSSL supporting Ed25519:

```bash
openssl pkeyutl -verify -pubin -inkey release-signing-key.pem -rawin -in update-manifest.json -sigfile update-manifest.sig
```

Success must report `Signature Verified Successfully`. A failure is not permission to extract or run the archive. Then compare the archive's byte count and SHA-256 with its matching record in the **verified** manifest. For example, replacing the filename with the actual downloaded asset:

```bash
openssl dgst -sha256 numera-your-platform-actual-source-sha.tar.gz
```

Windows can also inspect the final executable's publisher signature:

```powershell
Get-AuthenticodeSignature -LiteralPath '.\Numera Browser\numera.exe'
```

A valid file signature does not make a self-signed certificate publicly trusted.
Windows policy may still require a publicly trusted build.

## Verify a versioned download

Download your package, its `.sig`, `SHA256SUMS` and `SHA256SUMS.sig` from the same
versioned release. Use a separately trusted copy of the checked-in
[publisher certificate](code-signing-cert.pem). A public key downloaded with an
archive is not an independent trust source.

Run these from the download directory. Replace the asset filename with the one
that you downloaded. Stop if any signature or checksum fails.

Bash:

```bash
set -euo pipefail
asset=numera-browser-0.37.0-linux-x64-portable.zip
openssl x509 -in code-signing-cert.pem -pubkey -noout -out trusted-publisher-key.pem
openssl dgst -sha256 -verify trusted-publisher-key.pem -signature SHA256SUMS.sig SHA256SUMS
openssl dgst -sha256 -verify trusted-publisher-key.pem -signature "$asset.sig" "$asset"
awk -v asset="$asset" '$2 == asset { print }' SHA256SUMS | sha256sum --check --strict
```

PowerShell 7 (with OpenSSL on `PATH`):

```powershell
$asset = 'numera-browser-0.37.0-windows-x64.msi'
openssl x509 -in code-signing-cert.pem -pubkey -noout -out trusted-publisher-key.pem
if ($LASTEXITCODE -ne 0) { throw 'Could not read the trusted certificate' }
openssl dgst -sha256 -verify trusted-publisher-key.pem -signature SHA256SUMS.sig SHA256SUMS
if ($LASTEXITCODE -ne 0) { throw 'Invalid checksum-list signature' }
openssl dgst -sha256 -verify trusted-publisher-key.pem -signature "$asset.sig" "$asset"
if ($LASTEXITCODE -ne 0) { throw 'Invalid package signature' }
$pattern = '^([0-9a-f]{64})  ' + [regex]::Escape($asset) + '$'
$entry = @(Select-String -LiteralPath SHA256SUMS -Pattern $pattern)
if ($entry.Count -ne 1 -or (Get-FileHash -LiteralPath $asset -Algorithm SHA256).Hash -ne $entry[0].Matches[0].Groups[1].Value) {
    throw 'Package checksum missing or mismatched'
}
Get-AuthenticodeSignature -LiteralPath $asset
```

Authenticode reports the Windows trust status separately. Even with valid
detached signatures and checksums, follow your local publisher trust policy.

## Data safety and recovery

Mutable data, browser profile storage, vault/configuration and explicit external resource overrides are not update-owned files. The updater stages beside the installation on the same volume and replaces only authenticated owned files, with an exclusive transaction and durable recovery journal. It does not rename an arbitrary nonempty application/data directory and assume the operation is atomic.

Root-level admission exclusion and installation-wide runtime leases prevent cooperating application instances using another data root from racing immutable file replacement. A closed or malformed admission/recovery marker is not released merely because it is old. New/rollback readiness is bound to the owned recovery identity; unrelated launches do not acquire that privilege.

Admission and private quiesce/resume failures return fixed diagnostic messages rather than raw filesystem exceptions. An active or uncertain owner still defers the update; a generic access error does not authorize clearing the gate. Cloak acquisition metadata likewise reports an unavailable offer without returning vendor exception text.

After interruption, preserve the staging/recovery state and diagnostic logs. Do not manually delete locks, gate files or the last working generation to make startup continue. Use the retained worker's `--numera-update-recover` command with its exact private journal path. If interruption happened before the worker was copied and the installed shell is unchanged, that shell accepts the same recovery command. Never attach the journal to a public issue: it contains a private recovery capability.

A durable admission commit is a one-way boundary: after it, recovery validates and resumes the committed generation, never automatically downgrades files after user work might have resumed. A matching hashed-ticket resume receipt makes a lost successful response safely retryable; it does not unlock a different later transaction. Before that commit, authenticated old files can be restored, and the journal stays pending until readiness and private resume succeed. Recovery moves must not replace an occupied destination: Linux/macOS use exclusive native renames, while Windows retains its non-replacing move. Unsupported filesystems refuse rather than fall back to an overwriting operation. The preservation regression passed current contracts on all three platforms. Linux/macOS also proved the exact previous implementation fails the occupied-destination assertion, restored fixed bytes and passed after selecting the owning library. These contracts do not certify actual updater activation. If ownership or file state is uncertain, recovery refuses and retains evidence. Application rollback does not roll back your browser databases automatically.

## Maintainer publication

The [release workflow](../.github/workflows/release.yml) runs Native CI on the
exact version tag before signing. All four Windows and Linux build legs must
pass. The protected `numera-signing` environment provides
`NUMERA_SIGNING_PFX_BASE64`, `NUMERA_SIGNING_PASSWORD` and
`NUMERA_RELEASE_SIGNING_KEY`. The publisher verifies the complete asset set
before creating the release. Manual artifact review cannot publish.

Build workflows pass `PLAYWRIGHT_SKIP_DRIVER_DOWNLOAD=1` in the environment.
From the repository root, run `cargo run --manifest-path tools/xtask/Cargo.toml --locked --`
with the required subcommand for local build review. See [the build-tool guide](../tools/xtask/README.md).
Local artifacts do not substitute for protected workflow signing.

Production-feature shell formatting, lint and unit contracts run in a separate Linux and Windows x86-64 hosted tier before signing/distribution, without release keys. Required verification and distribution both depend on that tier; those unit tests are not repeated during packaging. Actual desktop activation/recovery and signing still require every original pre-distribution gate, including the public CodeQL verdict.

Hosted updater acceptance compiles two separate shell and frozen-engine generations with an immutable synthetic verification key. Those binaries explicitly identify themselves as acceptance-only; production identity verification and packaging reject them. The test uses the actual hidden desktop, private engine bridge, copied worker and interruption recovery, not a rewritten manifest around one unchanged executable. To fit bounded hosted storage it carries the real browser compatibility manifest and one hashed bundled resource, not duplicate full browser installations. Full browser packaging/launch is a separate product check; the compact updater fixture does not prove full-browser archive space or performance. A staged-substitution case re-signs only a synthetic BUILD version after download and invokes the actual worker after quiescence; it requires refusal before replacement while retaining the matching recovery journal/admission and old files/data. That refusal is not evidence of completed recovery or a usable resumed application. Linux uses an isolated virtual display; missing platform WebView capability fails acceptance rather than being replaced by a mock.

Only the named authorized Actions signing inputs are used. Untrusted code/ref inputs do not receive production keys. Ed25519 metadata signing uses the validated key directly in memory; it does not stage an additional private PEM file or pass it to a signing subprocess. Native platform signing still uses restricted temporary key material where the platform tool requires it. Private signing keys, passwords, temporary keychains and recovery credentials are not source or downloadable artifacts. A signature failure blocks publication; an unsigned output is not a completed release.

Key loss/rotation requires an explicitly authorized trust migration. A public key substituted in a feed cannot repair clients pinned to the old key. Preserve protected local recovery custody and never attach private signing inputs to an issue.
