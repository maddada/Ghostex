//! Reads which CEF build a Linux `libcef.so` is without loading it.

use std::{
    fs::File,
    io::{Read as _, Seek as _, SeekFrom},
    path::Path,
    sync::OnceLock,
};

/// The largest `.rodata` this reads (CEF 156's is about 20 MB on arm64 and x64).
const MAX_RODATA_BYTES: u64 = 256 * 1024 * 1024;

/// The CEF version string compiled into `library` (for example
/// `154.0.26+ge72305f+chromium-154.0.8037.58`, what `cef_version_full()` returns), or `None` when
/// its read-only data holds none.
///
/// CDXC:CefRuntime 2026-10-27 WHY:
/// A libcef.so from another CEF build aborts the process (`LOG(FATAL)` "CefApp_0_CToCpp called with invalid version -1" inside `cef_execute_process`) the moment this build asks it for its API version, so a runtime folder outside the component store is checked before anything loads it (GitHub #339: a CEF 154 payload left in /opt/ghostex by an older flat install stopped 10.36.0, which needs CEF 156, on every launch). Asking the library itself means loading it, which runs its initializers and costs a probe process per launch; the version string `cef_version_full()` returns is a constant in the library's `.rodata`, so it is read from there: one section of about 20 MB instead of a 260 MB (stripped) to 2.5 GB (unstripped development copy) file.
pub(crate) fn libcef_build_version(library: &Path) -> Result<Option<String>, String> {
    let mut file = File::open(library)
        .map_err(|error| format!("Could not open {}: {error}", library.display()))?;
    let rodata = read_elf_section(&mut file, ".rodata")
        .map_err(|error| format!("Could not read {}: {error}", library.display()))?
        .ok_or_else(|| format!("{} has no read-only data section.", library.display()))?;
    Ok(cef_version_in(&rodata))
}

/// The first CEF version string in `bytes`.
fn cef_version_in(bytes: &[u8]) -> Option<String> {
    static PATTERN: OnceLock<regex::bytes::Regex> = OnceLock::new();
    let pattern = PATTERN.get_or_init(|| {
        regex::bytes::Regex::new(
            r"(?-u)[0-9]+\.[0-9]+\.[0-9]+\+g[0-9a-f]+\+chromium-[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+",
        )
        .expect("the CEF version pattern must compile")
    });
    pattern
        .find(bytes)
        .map(|found| String::from_utf8_lossy(found.as_bytes()).into_owned())
}

/// The bytes of the section called `name` in a 64-bit little-endian ELF file (the only kind the
/// Linux builds ship: x86_64 and aarch64).
fn read_elf_section(file: &mut File, name: &str) -> Result<Option<Vec<u8>>, String> {
    let mut header = [0u8; 64];
    file.read_exact(&mut header)
        .map_err(|_| "it is not an ELF library".to_string())?;
    if header[..4] != *b"\x7fELF" || header[4] != 2 || header[5] != 1 {
        return Err("it is not a 64-bit little-endian ELF library".to_string());
    }
    let section_table_offset = u64_at(&header, 0x28);
    let section_header_size = u16_at(&header, 0x3a) as u64;
    let section_count = u16_at(&header, 0x3c) as u64;
    let names_index = u16_at(&header, 0x3e) as u64;
    if section_header_size < 64 || names_index >= section_count {
        return Err("its section table is malformed".to_string());
    }
    let table = read_at(
        file,
        section_table_offset,
        section_header_size * section_count,
    )?;
    let section = |index: u64| {
        let start = (index * section_header_size) as usize;
        let entry = &table[start..start + 64];
        (
            u32_at(entry, 0) as usize,
            u64_at(entry, 0x18),
            u64_at(entry, 0x20),
        )
    };
    let (_, names_offset, names_size) = section(names_index);
    let names = read_at(file, names_offset, names_size.min(1024 * 1024))?;
    for index in 0..section_count {
        let (name_offset, offset, size) = section(index);
        let Some(section_name) = names.get(name_offset..).and_then(|rest| {
            rest.iter()
                .position(|byte| *byte == 0)
                .map(|end| &rest[..end])
        }) else {
            continue;
        };
        if section_name != name.as_bytes() {
            continue;
        }
        if size > MAX_RODATA_BYTES {
            return Err(format!("its {name} section is {size} bytes"));
        }
        return read_at(file, offset, size).map(Some);
    }
    Ok(None)
}

fn read_at(file: &mut File, offset: u64, length: u64) -> Result<Vec<u8>, String> {
    let mut bytes = vec![0u8; length as usize];
    file.seek(SeekFrom::Start(offset))
        .and_then(|_| file.read_exact(&mut bytes))
        .map_err(|error| format!("it is truncated ({error})"))?;
    Ok(bytes)
}

fn u16_at(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().expect("four bytes"))
}

fn u64_at(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().expect("eight bytes"))
}

/// A minimal 64-bit little-endian ELF file with a `.text` section and a `.rodata` section holding
/// `rodata`, the shape `libcef_build_version` reads.
#[cfg(test)]
pub(crate) fn fake_elf_library(rodata: &[u8]) -> Vec<u8> {
    let text = b"\x1f\x20\x03\xd5 not the version 1.2.3+gabcdef0+chromium-1.2.3.4";
    let names = b"\0.text\0.rodata\0.shstrtab\0";
    let text_offset = 64u64;
    let rodata_offset = text_offset + text.len() as u64;
    let names_offset = rodata_offset + rodata.len() as u64;
    let table_offset = names_offset + names.len() as u64;
    let mut file = vec![0u8; 64];
    file[..6].copy_from_slice(b"\x7fELF\x02\x01");
    file[0x28..0x30].copy_from_slice(&table_offset.to_le_bytes());
    file[0x3a..0x3c].copy_from_slice(&64u16.to_le_bytes());
    file[0x3c..0x3e].copy_from_slice(&4u16.to_le_bytes());
    file[0x3e..0x40].copy_from_slice(&3u16.to_le_bytes());
    file.extend_from_slice(text);
    file.extend_from_slice(rodata);
    file.extend_from_slice(names);
    let mut entry = |name: u32, offset: u64, size: u64| {
        let mut header = [0u8; 64];
        header[..4].copy_from_slice(&name.to_le_bytes());
        header[0x18..0x20].copy_from_slice(&offset.to_le_bytes());
        header[0x20..0x28].copy_from_slice(&size.to_le_bytes());
        file.extend_from_slice(&header);
    };
    entry(0, 0, 0);
    entry(1, text_offset, text.len() as u64);
    entry(7, rodata_offset, rodata.len() as u64);
    entry(15, names_offset, names.len() as u64);
    file
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_library(name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ghostex-cef-library-version-{}-{name}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("create the test folder");
        let path = dir.join("libcef.so");
        std::fs::write(&path, bytes).expect("write the test library");
        path
    }

    #[test]
    fn reads_the_version_from_rodata_only() {
        let library = write_library(
            "rodata",
            &fake_elf_library(b"\0CEF\0154.0.26+ge72305f+chromium-154.0.8037.58\0more\0"),
        );
        assert_eq!(
            libcef_build_version(&library).unwrap().as_deref(),
            Some("154.0.26+ge72305f+chromium-154.0.8037.58")
        );
        std::fs::remove_dir_all(library.parent().unwrap()).unwrap();
    }

    #[test]
    fn a_library_without_a_version_string_has_none() {
        let library = write_library("none", &fake_elf_library(b"\0no version here\0"));
        assert_eq!(libcef_build_version(&library).unwrap(), None);
        std::fs::remove_dir_all(library.parent().unwrap()).unwrap();
    }

    #[test]
    fn a_file_that_is_not_elf_is_an_error() {
        let library = write_library(
            "not-elf",
            b"MZ this is not an ELF library at all, padded to 64 bytes......",
        );
        assert!(libcef_build_version(&library).is_err());
        std::fs::remove_dir_all(library.parent().unwrap()).unwrap();
    }

    /// The published CEF component, when GHOSTEX_TEST_LIBCEF points at its libcef.so and
    /// GHOSTEX_TEST_LIBCEF_VERSION names its version.
    #[test]
    fn reads_a_real_libcef_when_one_is_given() {
        let (Some(library), Some(version)) = (
            std::env::var_os("GHOSTEX_TEST_LIBCEF"),
            std::env::var("GHOSTEX_TEST_LIBCEF_VERSION").ok(),
        ) else {
            return;
        };
        let started = std::time::Instant::now();
        let found = libcef_build_version(Path::new(&library)).unwrap();
        eprintln!("read {found:?} in {:?}", started.elapsed());
        assert_eq!(found.as_deref(), Some(version.as_str()));
    }
}
