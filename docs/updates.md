# Portable updates and release verification

## Status and scope

The updater implements consent-gated **complete portable release generations**: the desktop shell, retained Python engine, dashboard, matching browser SDK/binary resources and required notices. Source checkouts, wheels and containers use their own package/deployment workflow; the updater does not modify them.

The implementation and publication pipeline are under hosted acceptance. Until the matching exact-source CI run and release assets have been verified, this document describes the protocol and intended shipped interface, **not a claim that the current tree has passed or a release exists**. Python remains the selected application runtime. This feature does not close the separate [Rust migration gates](migration.md).

## End-user flow

1. The desktop checks the project's GitHub release feed after readiness and at most once every 24 hours while open. **Check for updates** requests a manual check. There is no operating-system background service.
2. **Settings → Software updates** shows the installed version/source/generation and the authenticated offer, download size, browser/SDK changes and restart requirement. An unsigned GitHub description is not an authenticated update offer.
3. **Download update → Approve download** approves only that specific offer. **Later** closes the prompt without downloading. Downloading alone never authorizes installation.
4. After verification, **Install and restart → Approve restart** closes admission, proves affected work idle, and starts a bounded activation worker. Active sessions, jobs or uncertain ownership defer installation; the updater does not force-stop them.
5. The new generation must prove its exact identity/readiness before recovery is committed and work admission reopens. Cancellation, unsupported prerequisites and errors are shown rather than hidden by automatic retries.

Keep enough writable space for the archive, expanded files and rollback. Managed files changed outside the supported release process can prevent verification. Do not bypass that refusal by deleting manifests or clearing ownership records.

### Browser updates

Playwright, Patchright and Camoufox SDKs and their binary revisions are bound to a tested release generation. A whole application update may therefore deliver a browser update too. The updater does **not** place arbitrary latest Chromium/Firefox under an older SDK. Explicit external browser paths are outside managed replacement.

CloakBrowser's proprietary browser binary is not redistributed. First acquisition is a separate approved vendor operation. The wrapper's background mutation is suppressed; missing resources do not grant implicit download consent. Free-key/latest-only or otherwise unsupported vendor version contracts cannot be advertised as an exact tested pin. Existing permitted local overrides remain user-managed.

## What is signed

Two trust layers have distinct purposes:

| Layer | What it proves | What it does not prove |
|---|---|---|
| Ed25519 release/BUILD signatures | Exact metadata bytes were signed by the pinned project key; hashes bind owned files and archives. | Public OS publisher trust, notarization, absence of bugs. |
| Native Windows/Apple signatures | Final executable bytes match the designated owner certificate. | Public trust for a self-signed certificate, automatic SmartScreen/Gatekeeper acceptance. |

The public manifest verification key is [release-signing-key.pem](release-signing-key.pem).
The native private-trust certificate is [code-signing-cert.pem](code-signing-cert.pem).
These are **public** verification material, not private keys. Obtain them from a trusted project checkout/distribution; a key downloaded from an untrusted mirror alongside an archive is not a trust anchor.

The current native certificate is self-signed. No Apple Developer ID/notarization or publicly trusted Windows certificate is claimed. Do not install it as a system root to suppress warnings. Key generation does not create a public certification entitlement.

## Manifest protocol 1

`update-manifest.sig` is a raw **64-byte Ed25519 signature** over the exact UTF-8 bytes of `update-manifest.json`. This is not a base64 Tauri/minisign signature. Do not reformat JSON before verification.

The manifest authenticates repository/product, version, full source SHA, positive monotonic release generation, creation/expiry times, minimum protocol, per-platform SDK/browser metadata and each approved archive's size/SHA-256, expanded bounds, member limit, root and BUILD hash. The generation uses the producing GitHub run identity, not lexicographic SHA comparison. Build prereleases are intentionally considered; `/releases/latest` alone would miss them.

Inside a portable archive, `BUILD.json` and raw `BUILD.sig` authenticate the initial installed file inventory and identity. Regular-file records include path/size/SHA-256/executable status; permitted symbolic links carry contained destinations. `PROVENANCE.json` holds extended build/browser/license information. BUILD cannot hash itself or its signature; the outer signed archive/BUILD hashes close that cycle.

Discovery permits only approved HTTPS GitHub release hosts, with bounded metadata, redirects and network deadlines. Downloads stream to disk; declared archive/expanded/member bounds, path/link containment and final file hashes are checked before live replacement. The pinned public key is never replaced by an unsigned feed response. An unknown protocol, wrong platform/SDK, bad signature, hash mismatch, stale/replayed generation or malformed inventory fails closed.

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

A self-signed chain can be untrusted even when the designated signer and cryptographic file signature are correct. Do not describe that as public OS trust. The release workflow independently checks the designated certificate and rejects changed/wrong-signer copies; end-user policy may still require a publicly trusted build.

## Data safety and recovery

Mutable data, browser profile storage, vault/configuration and explicit external resource overrides are not update-owned files. The updater stages beside the installation on the same volume and replaces only authenticated owned files, with an exclusive transaction and durable recovery journal. It does not rename an arbitrary nonempty application/data directory and assume the operation is atomic.

Root-level admission exclusion and installation-wide runtime leases prevent cooperating application instances using another data root from racing immutable file replacement. A closed or malformed admission/recovery marker is not released merely because it is old. New/rollback readiness is bound to the owned recovery identity; unrelated launches do not acquire that privilege.

Admission and private quiesce/resume failures return fixed diagnostic messages rather than raw filesystem exceptions. An active or uncertain owner still defers the update; a generic access error does not authorize clearing the gate. Cloak acquisition metadata likewise reports an unavailable offer without returning vendor exception text.

After interruption, preserve the staging/recovery state and diagnostic logs. Do not manually delete locks, gate files or the last working generation to make startup continue. Use the retained worker's `--numera-update-recover` command with its exact private journal path. If interruption happened before the worker was copied and the installed shell is unchanged, that shell accepts the same recovery command. Never attach the journal to a public issue: it contains a private recovery capability.

A durable admission commit is a one-way boundary: after it, recovery validates and resumes the committed generation, never automatically downgrades files after user work might have resumed. A matching hashed-ticket resume receipt makes a lost successful response safely retryable; it does not unlock a different later transaction. Before that commit, authenticated old files can be restored, and the journal stays pending until readiness and private resume succeed. If ownership or file state is uncertain, recovery refuses and retains evidence. Application rollback does not roll back your browser databases automatically.

## Maintainer publication

The owning repository's manually dispatched CI validates one exact reviewed SHA. Final platform bytes are signed **after** their last content mutation and before archiving. BUILD signatures, archive hashes, provenance and negative signature checks travel with that build. The same batch's publisher runs only after required validation succeeds and only for its trusted main-branch publication target. It reuses verified artifacts, never rebuilds a different source after the gate.

Hosted updater acceptance compiles two separate shell and frozen-engine generations with an immutable synthetic verification key. Those binaries explicitly identify themselves as acceptance-only; production identity verification and packaging reject them. The test uses the actual hidden desktop, private engine bridge, copied worker and interruption recovery, not a rewritten manifest around one unchanged executable. To fit bounded hosted storage it carries the real browser compatibility manifest and one hashed bundled resource, not duplicate full browser installations. Full browser packaging/launch is a separate product check; the compact updater fixture does not prove full-browser archive space or performance. Linux uses an isolated virtual display; missing platform WebView capability fails acceptance rather than being replaced by a mock.

Only the named authorized Actions signing inputs are used. Untrusted code/ref inputs do not receive production keys. Ed25519 metadata signing uses the validated key directly in memory; it does not stage an additional private PEM file or pass it to a signing subprocess. Native platform signing still uses restricted temporary key material where the platform tool requires it. Private signing keys, passwords, temporary keychains and recovery credentials are not source or downloadable artifacts. A signature failure blocks publication; an unsigned output is not a completed release.

Key loss/rotation requires an explicitly authorized trust migration. A public key substituted in a feed cannot repair clients pinned to the old key. Preserve protected local recovery custody and never attach private signing inputs to an issue.
