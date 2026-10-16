"""Install a Chrome Web Store extension from its URL.

Chromium cannot install from the Web Store while automation is enabled, and
``--load-extension`` only accepts an *unpacked* directory. Every commercial
anti-detect browser still lets you paste a store link, and they do it the same
way this does: fetch the packed ``.crx`` from Google's update endpoint, unpack
it, and hand the browser the resulting folder.

A ``.crx`` is a small header followed by an ordinary zip, so unpacking needs no
dependency — just the header length and ``zipfile``.

Extensions land in ``<data-dir>/extensions/<id>/`` so they travel with a
portable install rather than pointing at somewhere on one machine.
"""

from __future__ import annotations

import re
import shutil
import struct
import urllib.request
import zipfile
from pathlib import Path
from typing import Optional

# Chrome extension ids are exactly 32 characters from a-p (a base-16 alphabet
# shifted into letters), which is why an id that "looks right" but is 31 chars
# is a truncated copy-paste rather than a different extension.
ID_RE = re.compile(r"\b([a-p]{32})\b")

# The endpoint Chrome itself uses to fetch updates. `prodversion` only has to be
# recent enough that the store does not refuse it.
CRX_URL = ("https://clients2.google.com/service/update2/crx"
           "?response=redirect&prodversion=131.0.0.0&acceptformat=crx2,crx3"
           "&x=id%3D{id}%26uc")

CRX_MAGIC = b"Cr24"
USER_AGENT = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/131.0.0.0"


class WebStoreError(RuntimeError):
    """Anything that stops a store URL becoming a loadable folder."""


def extension_id(url_or_id: str) -> str:
    """Pull the extension id out of a Web Store URL, or accept a bare id.

    Raises with a specific message when the id is *nearly* right: a 31-character
    run is the single most common failure, because selecting the id in a browser
    address bar drops a character surprisingly easily, and 'not found' would send
    the user looking in the wrong place."""
    text = (url_or_id or "").strip()
    if not text:
        raise WebStoreError("no URL or extension id given")

    m = ID_RE.search(text)
    if m:
        ext_id = m.group(1)
        if re.fullmatch(r"[a-p]{32}", ext_id) is None:
            raise WebStoreError("invalid Chrome extension id")
        return ext_id

    # A near-miss deserves a specific message. Two things go wrong in practice and
    # they need different advice: an id the right shape but the wrong length is a
    # truncated copy, while one containing letters past 'p' is not an id at all.
    near = re.search(r"/detail/[^/]+/([a-z]{20,40})", text) or re.search(r"\b([a-z]{20,40})\b", text)
    if near:
        found = near.group(1)
        bad = sorted({c for c in found if not ("a" <= c <= "p")})
        if bad:
            raise WebStoreError(
                f"'{found}' contains {', '.join(bad)}, which cannot appear in a Chrome "
                "extension id - ids use only the letters a-p. This usually means the "
                "address was mistyped or partly copied; open the extension's page and "
                "copy the whole URL from the address bar.")
        raise WebStoreError(
            f"'{found}' is {len(found)} characters; a Chrome extension id is exactly 32. "
            "It looks like the id was copied incompletely - open the extension's page "
            "and copy the whole URL from the address bar.")
    raise WebStoreError(
        f"could not find an extension id in {text!r}. Paste the full Web Store URL, "
        "or the 32-character id on its own.")


def _download(ext_id: str, timeout: int = 60) -> bytes:
    req = urllib.request.Request(CRX_URL.format(id=ext_id),
                                 headers={"User-Agent": USER_AGENT})
    try:
        with urllib.request.urlopen(req, timeout=timeout) as r:
            data = r.read()
    except Exception as e:                       # network, DNS, HTTP error
        raise WebStoreError(f"could not download {ext_id}: {e}") from e

    if not data.startswith(CRX_MAGIC):
        # A 404 from the store is an HTML page, not a crx. Saying so is more
        # useful than letting zipfile fail with "not a zip file" later.
        raise WebStoreError(
            f"the store did not return an extension for id '{ext_id}' - check the id "
            "is correct and the extension is still published.")
    return data


def _extract_safely(z: zipfile.ZipFile, dest: Path) -> None:
    """Extract every member, refusing any that would land outside `dest`.

    The previous guard compared strings, so `<dest>-evil/x` passed for starting
    with `<dest>`. `extractall` is not used: it would resolve members itself and
    the check would no longer describe what actually gets written.
    """
    from .paths import safe_join

    members = [i for i in z.infolist() if not i.is_dir()]
    try:
        targets = [(i, safe_join(dest, i.filename)) for i in members]   # raises first
    except ValueError as e:
        raise WebStoreError(f"refusing archive member outside the folder: {e}") from e
    for info, target in targets:
        target.parent.mkdir(parents=True, exist_ok=True)
        with z.open(info) as fh:
            target.write_bytes(fh.read())


def _unpack_zip(data: bytes, dest: Path) -> None:
    """The plain-zip half of `_unpack`, so the containment rule is testable."""
    import io as _io

    try:
        with zipfile.ZipFile(_io.BytesIO(data)) as z:
            _extract_safely(z, dest)
    except zipfile.BadZipFile as e:
        raise WebStoreError(f"the downloaded file is not a valid extension package: {e}") from e


def _icon_path(folder: Path, rel: str) -> "Optional[Path]":
    """An icon path from a third-party manifest, or None if it is not usable."""
    from .paths import safe_join

    try:
        candidate = safe_join(folder, rel)
    except ValueError:
        return None
    return candidate if candidate.is_file() else None


def _unpack(crx: bytes, dest: Path) -> None:
    """Strip the CRX header and extract the zip behind it.

    CRX3: magic(4) version(4) header_len(4) header(header_len) zip.
    CRX2 differs, and is long dead in the store, but costs three lines to keep
    working for anything still served in the old format."""
    version = struct.unpack("<I", crx[4:8])[0]
    if version == 3:
        header_len = struct.unpack("<I", crx[8:12])[0]
        start = 12 + header_len
    elif version == 2:
        pk_len, sig_len = struct.unpack("<II", crx[8:16])
        start = 16 + pk_len + sig_len
    else:
        raise WebStoreError(f"unsupported CRX version {version}")

    import io
    try:
        with zipfile.ZipFile(io.BytesIO(crx[start:])) as z:
            # Refuse path traversal before writing anything: the archive is
            # third-party content, and a member named ../../x would otherwise
            # escape the destination.
            _extract_safely(z, dest)
    except zipfile.BadZipFile as e:
        raise WebStoreError(f"the downloaded file is not a valid extension package: {e}") from e


def install(url_or_id: str, data_dir: Path, *, force: bool = False) -> dict:
    """Download and unpack a store extension. Returns its id, name and folder.

    Idempotent: an extension already installed is returned as-is unless `force`.
    """
    ext_id = extension_id(url_or_id)
    root = Path(data_dir) / "extensions"
    dest = root / ext_id

    if dest.is_dir() and (dest / "manifest.json").is_file() and not force:
        return {"id": ext_id, "path": str(dest), "name": _name(dest), "reused": True}

    crx = _download(ext_id)
    if dest.exists():
        shutil.rmtree(dest, ignore_errors=True)
    dest.mkdir(parents=True, exist_ok=True)
    try:
        _unpack(crx, dest)
    except Exception:
        shutil.rmtree(dest, ignore_errors=True)   # never leave a half-unpacked folder
        raise

    if not (dest / "manifest.json").is_file():
        shutil.rmtree(dest, ignore_errors=True)
        raise WebStoreError(
            f"'{ext_id}' unpacked without a manifest.json, so Chromium would refuse it.")

    return {"id": ext_id, "path": str(dest), "name": _name(dest), "reused": False}


def _name(folder: Path) -> str:
    """The extension's display name, best-effort.

    Chrome stores localised names as `__MSG_key__`; resolving those means reading
    _locales, which is more machinery than a label is worth. Fall back to the id
    when it cannot be read plainly."""
    import json
    try:
        manifest = json.loads((folder / "manifest.json").read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return folder.name
    name = str(manifest.get("name") or "")
    if name.startswith("__MSG_"):
        key = name[6:-2] if name.endswith("__") else name[6:]
        for locale in ("en", "en_US", manifest.get("default_locale") or ""):
            f = folder / "_locales" / locale / "messages.json"
            if f.is_file():
                try:
                    msgs = json.loads(f.read_text(encoding="utf-8"))
                    got = msgs.get(key, {}).get("message")
                    if got:
                        return got
                except (OSError, json.JSONDecodeError):
                    pass
        return folder.name
    return name or folder.name


def icon(folder: Path) -> Optional[Path]:
    """The extension's own icon file, largest size available.

    Chrome puts it in `icons` keyed by pixel size; MV2 extensions sometimes only
    declare one under `browser_action`/`page_action`. Returns None rather than a
    guess when the manifest names a file that is not actually there - a broken
    <img> is worse than a fallback glyph.
    """
    import json
    folder = Path(folder)
    try:
        manifest = json.loads((folder / "manifest.json").read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return None

    icons = manifest.get("icons")
    if not isinstance(icons, dict):
        icons = {}
    for action in ("action", "browser_action", "page_action"):
        default = (manifest.get(action) or {}).get("default_icon") if isinstance(manifest.get(action), dict) else None
        if isinstance(default, dict):
            icons = {**default, **icons}       # `icons` wins; the action is the fallback
        elif isinstance(default, str):
            icons.setdefault("0", default)

    def size(key: str) -> int:
        try:
            return int(key)
        except ValueError:
            return 0

    for key in sorted(icons, key=size, reverse=True):
        rel = icons[key]
        if not isinstance(rel, str) or not rel:
            continue
        # The manifest is third-party text; a member like ../../secret.png must
        # not resolve outside the extension folder.
        candidate = _icon_path(folder, rel)
        if candidate:
            return candidate
    return None


def describe(folder: Path) -> dict:
    """Name and icon for one unpacked extension folder, for the UI to render."""
    folder = Path(folder)
    ico = icon(folder)
    return {"path": str(folder), "name": _name(folder),
            "icon": str(ico) if ico else None}


def installed(data_dir: Path) -> list[dict]:
    """Every extension installed from the store into this data directory."""
    root = Path(data_dir) / "extensions"
    if not root.is_dir():
        return []
    out = []
    for d in sorted(root.iterdir()):
        if (d / "manifest.json").is_file():
            out.append({"id": d.name, **describe(d)})
    return out


def is_store_url(text: str) -> bool:
    t = (text or "").strip().lower()
    return "chromewebstore.google.com" in t or "chrome.google.com/webstore" in t


def uninstall(ext_id_or_url: str, data_dir: Path) -> bool:
    ext_id = extension_id(ext_id_or_url)
    dest = Path(data_dir) / "extensions" / ext_id
    if not dest.is_dir():
        return False
    shutil.rmtree(dest, ignore_errors=True)
    return not dest.exists()
