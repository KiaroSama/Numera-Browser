//! Assemble headless release archives and validate native release executables.
use flate2::{Compression, write::GzEncoder};
use std::{
    collections::BTreeMap,
    error::Error,
    ffi::OsString,
    fs::{self, OpenOptions},
    io,
    path::{Path, PathBuf},
};
use zip::{ZipWriter, write::SimpleFileOptions};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const APPROVED_ICON: &[u8] = include_bytes!("../../../desktop/src-tauri/icons/icon.ico");
const README: &[u8] = b"Numera Browser headless server\n\nKeep both executables together and put their directory on PATH.\n\nStart the headless HTTP server (no GUI or WebView is needed):\n  numera-engine serve --host 127.0.0.1 --port 8787\n\nGet machine-readable output from a one-shot command:\n  numera-engine engines --json\n\nUse either command as an AI agent's stdio MCP server:\n  numera-engine mcp\n  numera-engine mcp --bridge http://127.0.0.1:8787 --token YOUR_API_TOKEN\n\nReplace YOUR_API_TOKEN with the running server's API token.\nDo not expose API or DevTools ports publicly. Browser cores are not included;\nconfigure a supported browser separately before launching a profile. Browser\nlaunch commands accept --headless. Run numera-engine --help for command syntax.\n";

#[derive(Clone, Copy)]
struct Target {
    name: &'static str,
    windows: bool,
    machine: u16,
}
impl Target {
    fn parse(value: &OsString) -> Result<Self> {
        let (name, windows, machine) =
            match value.to_str() {
                Some("windows-x64") => ("windows-x64", true, 0x8664),
                Some("windows-arm64") => ("windows-arm64", true, 0xaa64),
                Some("linux-x64") => ("linux-x64", false, 62),
                Some("linux-arm64") => ("linux-arm64", false, 183),
                Some("macos-arm64") => ("macos-arm64", false, 0),
                _ => return Err(
                    "unsupported target; use windows-x64, windows-arm64, linux-x64, linux-arm64, or macos-arm64"
                        .into(),
                ),
            };
        Ok(Self {
            name,
            windows,
            machine,
        })
    }

    fn names(self) -> [&'static str; 2] {
        if self.windows {
            ["numera-engine.exe", "numera-browser-runtime.exe"]
        } else {
            ["numera-engine", "numera-browser-runtime"]
        }
    }
}

fn options(args: impl Iterator<Item = OsString>) -> Result<BTreeMap<String, OsString>> {
    let mut flags = BTreeMap::new();
    let mut args = args;
    while let Some(key) = args.next() {
        let key = key.into_string().map_err(|_| "flag must be UTF-8")?;
        let value = args.next().ok_or("missing flag value")?;
        if flags.insert(key, value).is_some() {
            return Err("duplicate flag".into());
        }
    }
    Ok(flags)
}

fn take(flags: &mut BTreeMap<String, OsString>, name: &str) -> Result<OsString> {
    flags
        .remove(name)
        .ok_or_else(|| format!("{name} is required").into())
}

pub fn verify(args: impl Iterator<Item = OsString>) -> Result<()> {
    let mut flags = options(args)?;
    let target = Target::parse(&take(&mut flags, "--target")?)?;
    let path = PathBuf::from(take(&mut flags, "--executable")?);
    if !flags.is_empty() {
        return Err("unexpected verify-binary flag".into());
    }
    read_binary(&path, target)?;
    println!("Verified {} for {}", path.display(), target.name);
    Ok(())
}

pub fn package(root: &Path, args: impl Iterator<Item = OsString>) -> Result<()> {
    let mut flags = options(args)?;
    let target = Target::parse(&take(&mut flags, "--target")?)?;
    let version = take(&mut flags, "--version")?
        .into_string()
        .map_err(|_| "version must be UTF-8")?;
    if !version.as_bytes().first().is_some_and(u8::is_ascii_digit)
        || !version
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b".+-".contains(&byte))
    {
        return Err("invalid package version".into());
    }
    let output = PathBuf::from(take(&mut flags, "--out")?);
    let binaries = if let Some(directory) = flags.remove("--bin-dir") {
        PathBuf::from(directory)
    } else {
        [root.join("bin"), root.join("engine-rust/target/release")]
            .into_iter()
            .find(|dir| target.names().iter().all(|name| dir.join(name).is_file()))
            .ok_or("engine and runtime binaries not found in bin/ or engine-rust/target/release; pass --bin-dir")?
    };
    if !flags.is_empty() {
        return Err("unexpected cli-package flag".into());
    }
    let [engine, runtime] = target.names();
    let engine_bytes = read_binary(&binaries.join(engine), target)?;
    let runtime_bytes = read_binary(&binaries.join(runtime), target)?;
    let license = fs::read(root.join("LICENSE"))?;
    let entries = [
        (engine, engine_bytes.as_slice()),
        (runtime, runtime_bytes.as_slice()),
        ("LICENSE", license.as_slice()),
        ("README-CLI.txt", README),
    ];
    let extension = if target.windows { "zip" } else { "tar.gz" };
    let path = output.join(format!(
        "numera-browser-cli-{version}-{}.{extension}",
        target.name
    ));
    fs::create_dir_all(output)?;
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    if let Err(error) =
        write_archive(file, target.windows, &entries).and_then(|file| Ok(file.sync_all()?))
    {
        fs::remove_file(&path)?;
        return Err(error);
    }
    println!("Wrote {}", path.display());
    Ok(())
}

fn write_archive<W: io::Write + io::Seek>(
    writer: W,
    windows: bool,
    entries: &[(&str, &[u8])],
) -> Result<W> {
    if windows {
        use io::Write;
        let mut zip = ZipWriter::new(writer);
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (name, bytes) in entries {
            zip.start_file(*name, options)?;
            zip.write_all(bytes)?;
        }
        Ok(zip.finish()?)
    } else {
        let mut tar = tar::Builder::new(GzEncoder::new(writer, Compression::default()));
        for (index, (name, bytes)) in entries.iter().enumerate() {
            let mut header = tar::Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(if index < 2 { 0o755 } else { 0o644 });
            tar.append_data(&mut header, name, *bytes)?;
        }
        Ok(tar.into_inner()?.finish()?)
    }
}

fn read_binary(path: &Path, target: Target) -> Result<Vec<u8>> {
    let metadata =
        fs::symlink_metadata(path).map_err(|error| format!("{}: {error}", path.display()))?;
    if !metadata.file_type().is_file() {
        return Err(format!("{} is not a regular executable", path.display()).into());
    }
    let bytes = fs::read(path)?;
    check_binary(&bytes, target).map_err(|error| format!("{}: {error}", path.display()))?;
    Ok(bytes)
}

fn check_binary(bytes: &[u8], target: Target) -> Result<()> {
    if target.windows {
        let resources = PeResources::read(bytes, target.machine)
            .ok_or("expected a matching PE32+ executable with a resource table")?;
        if !resources.has_approved_icon().unwrap_or(false) {
            return Err("Windows executable is missing its approved icon resource".into());
        }
    } else if target.name == "macos-arm64" {
        macho_arm64(bytes).ok_or("expected a bounded thin ARM64 Mach-O executable")?;
    } else if bytes.len() < 64
        || !bytes.starts_with(b"\x7fELF\x02\x01\x01")
        || !matches!(word(bytes, 16), Some(2 | 3))
        || word(bytes, 18) != Some(target.machine)
        || long(bytes, 20) != Some(1)
        || word(bytes, 52) != Some(64)
    {
        return Err("expected a matching little-endian ELF64 executable".into());
    }
    Ok(())
}

// Release binaries are native thin ARM64 executables. Universal containers,
// dylibs and truncated load commands must not pass an architecture-only check.
fn macho_arm64(bytes: &[u8]) -> Option<()> {
    if bytes.len() < 32
        || long(bytes, 0)? != 0xfeedfacf
        || long(bytes, 4)? != 0x0100000c
        || long(bytes, 8)? != 0
        || long(bytes, 12)? != 2
        || long(bytes, 28)? != 0
    {
        return None;
    }
    let count = usize::try_from(long(bytes, 16)?).ok()?;
    let size = usize::try_from(long(bytes, 20)?).ok()?;
    let end = 32usize.checked_add(size)?;
    if count == 0 || count > 4096 || size > 1024 * 1024 || count > size / 8 || end > bytes.len() {
        return None;
    }
    let mut cursor = 32usize;
    let mut entry = None;
    let mut executable_ranges = Vec::new();
    for _ in 0..count {
        let kind = long(bytes, cursor)?;
        let length = usize::try_from(long(bytes, cursor.checked_add(4)?)?).ok()?;
        let next = cursor.checked_add(length)?;
        if length < 8 || !length.is_multiple_of(8) || next > end {
            return None;
        }
        match kind {
            0x19 => {
                if length < 72 {
                    return None;
                }
                let sections = usize::try_from(long(bytes, cursor + 64)?).ok()?;
                if 72usize.checked_add(sections.checked_mul(80)?)? != length {
                    return None;
                }
                let file_offset = quad(bytes, cursor + 40)?;
                let file_size = quad(bytes, cursor + 48)?;
                let file_end = file_offset.checked_add(file_size)?;
                if file_end > bytes.len() as u64 || file_size > quad(bytes, cursor + 32)? {
                    return None;
                }
                if long(bytes, cursor + 60)? & 4 != 0 {
                    executable_ranges.push((file_offset, file_end));
                }
            }
            0x80000028 => {
                if length != 24 || entry.is_some() {
                    return None;
                }
                entry = Some(quad(bytes, cursor + 8)?);
            }
            _ => {}
        }
        cursor = next;
    }
    let entry = entry?;
    (cursor == end
        && entry >= end as u64
        && executable_ranges
            .iter()
            .any(|(start, end)| *start <= entry && entry < *end))
    .then_some(())
}

fn quad(bytes: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from_le_bytes(
        bytes.get(offset..offset.checked_add(8)?)?.try_into().ok()?,
    ))
}

fn word(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        bytes.get(offset..offset.checked_add(2)?)?.try_into().ok()?,
    ))
}

fn long(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset.checked_add(4)?)?.try_into().ok()?,
    ))
}

fn icon_images(ico: &[u8]) -> Option<Vec<&[u8]>> {
    if ico.get(..4)? != b"\0\0\x01\0" {
        return None;
    }
    (0..usize::from(word(ico, 4)?))
        .map(|i| {
            let size = long(ico, 14 + i * 16)? as usize;
            let offset = long(ico, 18 + i * 16)? as usize;
            ico.get(offset..offset.checked_add(size)?)
        })
        .collect()
}

struct PeResources<'a> {
    bytes: &'a [u8],
    sections: &'a [u8],
    table: &'a [u8],
}
impl<'a> PeResources<'a> {
    fn read(bytes: &'a [u8], machine: u16) -> Option<Self> {
        let pe = long(bytes, 0x3c)? as usize;
        let header = bytes.get(pe..)?.get(..24)?;
        if !bytes.starts_with(b"MZ")
            || !header.starts_with(b"PE\0\0")
            || word(header, 4)? != machine
            || word(header, 22)? & 0x2002 != 2
        {
            return None;
        }
        let (optional, tail) = bytes
            .get(pe + 24..)?
            .split_at_checked(word(header, 20)? as usize)?;
        if word(optional, 0)? != 0x20b || long(optional, 108)? < 3 {
            return None;
        }
        let sections = tail.get(..usize::from(word(header, 6)?) * 40)?;
        let table = Self::rva(bytes, sections, long(optional, 128)?, long(optional, 132)?)?;
        Some(Self {
            bytes,
            sections,
            table,
        })
    }

    fn rva(bytes: &'a [u8], sections: &[u8], rva: u32, size: u32) -> Option<&'a [u8]> {
        if rva == 0 || size == 0 {
            return None;
        }
        for section in sections.as_chunks::<40>().0 {
            let Some(delta) = rva.checked_sub(long(section, 12)?) else {
                continue;
            };
            if delta.checked_add(size)? <= long(section, 16)? {
                let start = long(section, 20)?.checked_add(delta)? as usize;
                return bytes.get(start..start.checked_add(size as usize)?);
            }
        }
        None
    }

    fn entries(&self, directory: u32) -> Option<&'a [u8]> {
        let tail = self.table.get((directory & 0x7fff_ffff) as usize..)?;
        let count = usize::from(word(tail, 12)?) + usize::from(word(tail, 14)?);
        tail.get(16..16 + count * 8)
    }

    fn entry(&self, directory: u32, id: u32) -> Option<u32> {
        self.entries(directory)?
            .as_chunks::<8>()
            .0
            .iter()
            .find(|entry| long(entry.as_slice(), 0) == Some(id))
            .and_then(|entry| long(entry.as_slice(), 4))
    }

    fn data(&self, directory: u32, language: u32) -> Option<&'a [u8]> {
        if directory & 0x8000_0000 == 0 {
            return None;
        }
        let leaf = self.entry(directory, language)?;
        if leaf & 0x8000_0000 != 0 {
            return None;
        }
        let data = self.table.get(leaf as usize..)?.get(..16)?;
        Self::rva(self.bytes, self.sections, long(data, 0)?, long(data, 4)?)
    }

    fn has_approved_icon(&self) -> Option<bool> {
        let icons = self.entry(0, 3)?;
        let groups = self.entry(0, 14)?;
        if icons & groups & 0x8000_0000 == 0 {
            return Some(false);
        }
        let approved = icon_images(APPROVED_ICON)?;
        // The first icon group is what Windows uses for the executable icon.
        let group = long(self.entries(groups)?, 4)?;
        for entry in self.entries(group)?.as_chunks::<8>().0 {
            let language = long(entry, 0)?;
            let data = self.data(group, language)?;
            if !data.starts_with(b"\0\0\x01\0") {
                return Some(false);
            }
            let count = usize::from(word(data, 4)?);
            if count == 0 || data.len() != 6 + count * 14 {
                return Some(false);
            }
            for image in data[6..].as_chunks::<14>().0 {
                let id = u32::from(word(image, 12)?);
                let raw = self.data(self.entry(icons, id)?, language)?;
                if raw.len() != long(image, 8)? as usize || !approved.contains(&raw) {
                    return Some(false);
                }
            }
        }
        Some(!self.entries(group)?.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Read};

    fn elf(machine: u16) -> Vec<u8> {
        let mut bytes = vec![0; 64];
        bytes[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
        bytes[16..18].copy_from_slice(&3u16.to_le_bytes());
        bytes[18..20].copy_from_slice(&machine.to_le_bytes());
        bytes[20] = 1;
        bytes[52] = 64;
        bytes
    }

    #[test]
    fn distinguishes_both_linux_targets_and_rejects_32_bit() -> Result<()> {
        for (name, machine) in [("linux-x64", 62u16), ("linux-arm64", 183)] {
            let target = Target::parse(&name.into())?;
            assert!(check_binary(&elf(machine), target).is_ok());
            assert!(check_binary(&elf(245 - machine), target).is_err());
            let mut wrong_class = elf(machine);
            wrong_class[4] = 1;
            assert!(check_binary(&wrong_class, target).is_err());
        }
        Ok(())
    }

    #[test]
    fn verifies_thin_macos_arm64_executables_and_rejects_malformed_commands() -> Result<()> {
        let target = Target::parse(&"macos-arm64".into())?;
        let mut bytes = vec![0u8; 192];
        for (offset, value) in [
            (0, 0xfeedfacfu32),
            (4, 0x0100000c),
            (12, 2),
            (16, 2),
            (20, 96),
            (32, 0x19),
            (36, 72),
            (64, 192),
            (80, 192),
            (92, 5),
            (104, 0x80000028),
            (108, 24),
            (112, 128),
        ] {
            bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        check_binary(&bytes, target)?;
        for (offset, value) in [
            (0, 0xcafebabeu32),
            (4, 0x01000007),
            (8, 2),
            (12, 6),
            (16, 0),
            (16, 4097),
            (20, u32::MAX),
            (28, 1),
            (36, 7),
            (36, 80),
            (80, 193),
            (92, 1),
            (96, 1),
            (108, 32),
            (112, 192),
        ] {
            let mut damaged = bytes.clone();
            damaged[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
            assert!(
                check_binary(&damaged, target).is_err(),
                "field {offset}={value}"
            );
        }
        for len in [0, 31, 63, 127, 191] {
            assert!(
                check_binary(&bytes[..len], target).is_err(),
                "truncated at {len}"
            );
        }
        assert!(Target::parse(&"macos-x64".into()).is_err());
        assert_eq!(target.names(), ["numera-engine", "numera-browser-runtime"]);
        Ok(())
    }

    #[test]
    fn requires_the_approved_pe_icon_for_each_windows_target() -> Result<()> {
        for name in ["windows-x64", "windows-arm64"] {
            let target = Target::parse(&name.into())?;
            let icon = icon_images(APPROVED_ICON).ok_or("invalid fixture ICO")?[0];
            let mut bytes = pe_with_icon(target.machine, icon);
            assert!(check_binary(&bytes, target).is_ok());
            bytes[0x370] ^= 1;
            assert!(check_binary(&bytes, target).is_err());
            bytes.truncate(0x200);
            assert!(check_binary(&bytes, target).is_err());
        }
        Ok(())
    }

    fn pe_with_icon(machine: u16, icon: &[u8]) -> Vec<u8> {
        let mut bytes = vec![0; 0x370 + icon.len()];
        bytes[..2].copy_from_slice(b"MZ");
        let put = |bytes: &mut [u8], offset, value: u32| {
            bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        };
        put(&mut bytes, 0x3c, 0x80);
        bytes[0x80..0x84].copy_from_slice(b"PE\0\0");
        put(&mut bytes, 0x84, (1 << 16) | u32::from(machine));
        put(&mut bytes, 0x94, (2 << 16) | 0xf0);
        put(&mut bytes, 0x98, 0x20b);
        put(&mut bytes, 0x104, 3);
        put(&mut bytes, 0x118, 0x1000);
        let section_size = (bytes.len() - 0x200) as u32;
        put(&mut bytes, 0x11c, section_size);
        put(&mut bytes, 0x190, section_size);
        put(&mut bytes, 0x194, 0x1000);
        put(&mut bytes, 0x198, section_size);
        put(&mut bytes, 0x19c, 0x200);
        for (base, ids) in [
            (0x200, &[(3, 0x8000_0030), (14, 0x8000_0050)][..]),
            (0x230, &[(1, 0x8000_0070)][..]),
            (0x250, &[(32512, 0x8000_0090)][..]),
            (0x270, &[(1033, 0xb0)][..]),
            (0x290, &[(1033, 0xc0)][..]),
        ] {
            put(&mut bytes, base + 12, (ids.len() as u32) << 16);
            for (index, &(id, target)) in ids.iter().enumerate() {
                put(&mut bytes, base + 16 + index * 8, id);
                put(&mut bytes, base + 20 + index * 8, target);
            }
        }
        put(&mut bytes, 0x2b0, 0x1170);
        put(&mut bytes, 0x2b4, icon.len() as u32);
        put(&mut bytes, 0x2c0, 0x1140);
        put(&mut bytes, 0x2c4, 20);
        bytes[0x340..0x346].copy_from_slice(b"\0\0\x01\0\x01\0");
        put(&mut bytes, 0x34e, icon.len() as u32);
        bytes[0x352] = 1;
        bytes[0x370..].copy_from_slice(icon);
        bytes
    }

    #[test]
    fn archives_contain_only_the_four_allowed_files() -> Result<()> {
        for windows in [false, true] {
            let names = if windows {
                [
                    "numera-engine.exe",
                    "numera-browser-runtime.exe",
                    "LICENSE",
                    "README-CLI.txt",
                ]
            } else {
                [
                    "numera-engine",
                    "numera-browser-runtime",
                    "LICENSE",
                    "README-CLI.txt",
                ]
            };
            let contents = [b"engine".as_slice(), b"runtime", b"license", README];
            let entries = names.into_iter().zip(contents).collect::<Vec<_>>();
            let data = write_archive(Cursor::new(Vec::new()), windows, &entries)?.into_inner();
            if windows {
                let mut archive = zip::ZipArchive::new(Cursor::new(data))?;
                assert_eq!(archive.len(), names.len());
                for (name, expected) in entries {
                    let mut actual = Vec::new();
                    archive.by_name(name)?.read_to_end(&mut actual)?;
                    assert_eq!(actual, expected);
                }
            } else {
                let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(data.as_slice()));
                let mut observed = Vec::new();
                for item in archive.entries()? {
                    let mut item = item?;
                    let mode = item.header().mode()?;
                    let path = item.path()?.into_owned();
                    let mut contents = Vec::new();
                    item.read_to_end(&mut contents)?;
                    observed.push((path, mode, contents));
                }
                assert_eq!(observed.len(), names.len());
                for (index, (path, mode, contents)) in observed.into_iter().enumerate() {
                    assert_eq!(path, Path::new(names[index]));
                    assert_eq!(mode, if index < 2 { 0o755 } else { 0o644 });
                    assert_eq!(contents, entries[index].1);
                }
            }
        }
        Ok(())
    }
}
