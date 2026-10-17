# Security Policy

## Reporting a vulnerability

If you find a security issue, please **do not** open a public issue. Instead,
email the maintainer or use GitHub's private "Report a vulnerability" feature
under the Security tab. Include steps to reproduce and the potential impact.

You can expect an acknowledgement within a few days and an update on the fix
timeline after triage.

## Scope

Numera Browser stores profile data under the resolved data root: explicit
`--data-dir`/`NUMERA_DATA_DIR`, writable portable `data/`, or the per-user fallback
(`~/.numera`, with existing pre-rename `~/.persona` retained). Settings → Storage
shows the actual path. Treat the whole root and its backups as sensitive.

Portable updates require pinned-key signed metadata and separate download and
installation consent. Native signatures currently use self-signed/private trust,
not public Windows trust or Apple notarization. Never clear uncertain recovery
markers or install a root certificate merely to bypass a refusal. See
[update trust and recovery](docs/updates.md).

Profile identifiers must be complete portable filenames; a linked profile/session
record is refused rather than resolved into another record. Archive paths receive
both lexical containment checks and physical resolution of the original member;
normalization must not erase links before the physical check. Atomic JSON writes
create a new temporary file exclusively, leaving any preexisting temporary file
untouched. New temporary files use owner-only mode on POSIX; that mode does not
establish a Windows ACL or encrypt the contents. Restrict access to the resolved
data root on every platform.

**Encrypting secrets:** proxy passwords can be encrypted at rest with a master
password — run `pip install -e ".[secure]"` then `numera vault enable`. This
covers every proxy password under the data directory: the engine's profiles, the
dashboard's copy of each one, and the shared proxy pool. Secrets
are stored as Fernet tokens (AES) with the key derived via PBKDF2-HMAC-SHA256;
supply the password at use time through `NUMERA_PASSWORD` or `--password`, so it
never touches disk. The rest of each profile stays readable. There is no password
recovery — losing it means losing the encrypted secrets. Encryption is opt-in:
with the vault disabled, stored proxy passwords remain plaintext. Private file
permissions do not replace enabling the vault.

**The proxy-authentication relay:** a profile whose proxy needs a username and
password is launched through a small local relay, because no browser engine can
authenticate every proxy type itself (Chromium refuses SOCKS authentication
outright). The relay listens on `127.0.0.1` on an ephemeral port, accepts no
authentication of its own, and holds the upstream credentials in memory for the
lifetime of that browser session.

That means any other process on the same machine that finds the port can use
your proxy while a profile is open. It cannot read the credentials, and the
port is never reachable from another machine, but on a shared or untrusted
machine this is worth knowing. The relay is started only when the proxy
actually carries credentials, and is torn down when the browser closes.

Not yet covered: cookies (Chromium keeps those in its own OS-encrypted store)
and full-profile encryption - the vault protects the secrets inside a profile,
not the file as a whole.
