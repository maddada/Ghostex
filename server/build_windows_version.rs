//! Writes a Windows `.res` file holding one VERSIONINFO resource, without rc.exe or any crate.
//!
//! CDXC:HelperNames 2026-11-21 WHY: a Windows program without version information shows as its
//! bare file name in Windows Defender Firewall's "Allow access" dialog, Defender alerts, Task
//! Manager and the Apps list ("gxserver.exe", "wmx.exe"), and people could not tell it was part of
//! Ghostex. A VERSIONINFO with FileDescription and ProductName makes them read "Ghostex Server",
//! "Ghostex Sessions" or "Ghostex CLI". The `.res` goes straight to link.exe
//! (`cargo:rustc-link-arg-bin`, as apps/desktop/build.rs links its own), so neither the server nor
//! wmx needs rc.exe or a new build dependency. The bytes match what llvm-rc writes for the same
//! VERSIONINFO script (checked 2026-11-21 with Homebrew llvm-rc). A copy of this file is in
//! .dependencies/wmx/build_windows_version.rs; keep the two the same.

/// The strings and version of one program.
pub struct VersionInfo<'a> {
    pub file_description: &'a str,
    pub product_name: &'a str,
    pub company_name: &'a str,
    pub internal_name: &'a str,
    pub original_filename: &'a str,
    /// Shown as written ("11.12.0").
    pub version_text: &'a str,
    /// The same version as four numbers for the fixed part.
    pub version: [u16; 4],
}

/// The numbers of a version such as `11.12.0` or `11.12.0-beta.1` (anything after `-` is dropped,
/// missing or unreadable parts are 0).
pub fn numeric_version(version: &str) -> [u16; 4] {
    let mut numbers = [0_u16; 4];
    let core = version.split_once('-').map_or(version, |(core, _)| core);
    for (index, part) in core.split('.').take(4).enumerate() {
        numbers[index] = part.trim().parse().unwrap_or(0);
    }
    numbers
}

/// The whole `.res` file: the empty resource every `.res` starts with, then RT_VERSION 1.
pub fn version_resource_file(info: &VersionInfo) -> Vec<u8> {
    let mut file = Vec::new();
    // The empty header resource.
    resource_header(&mut file, 0, 0, 0, 0, 0);
    let data = version_info_block(info);
    // RT_VERSION (16), id 1, MOVEABLE | PURE, US English.
    resource_header(&mut file, data.len() as u32, 16, 1, 0x0030, 0x0409);
    file.extend_from_slice(&data);
    pad_to_dword(&mut file);
    file
}

fn resource_header(
    out: &mut Vec<u8>,
    data_size: u32,
    kind: u16,
    name: u16,
    flags: u16,
    language: u16,
) {
    out.extend_from_slice(&data_size.to_le_bytes());
    out.extend_from_slice(&32_u32.to_le_bytes());
    for ordinal in [kind, name] {
        out.extend_from_slice(&0xFFFF_u16.to_le_bytes());
        out.extend_from_slice(&ordinal.to_le_bytes());
    }
    out.extend_from_slice(&0_u32.to_le_bytes()); // DataVersion
    out.extend_from_slice(&flags.to_le_bytes());
    out.extend_from_slice(&language.to_le_bytes());
    out.extend_from_slice(&0_u32.to_le_bytes()); // Version
    out.extend_from_slice(&0_u32.to_le_bytes()); // Characteristics
}

fn version_info_block(info: &VersionInfo) -> Vec<u8> {
    let [a, b, c, d] = info.version.map(u32::from);
    let mut fixed = Vec::new();
    for value in [
        0xFEEF_04BD_u32, // signature
        0x0001_0000,     // structure version
        (a << 16) | b,   // file version
        (c << 16) | d,
        (a << 16) | b, // product version
        (c << 16) | d,
        0x3F,    // flags mask
        0,       // flags
        0x40004, // VOS_NT_WINDOWS32
        1,       // VFT_APP
        0,       // subtype
        0,       // date
        0,
    ] {
        fixed.extend_from_slice(&value.to_le_bytes());
    }

    let strings: Vec<Vec<u8>> = [
        ("CompanyName", info.company_name),
        ("FileDescription", info.file_description),
        ("FileVersion", info.version_text),
        ("InternalName", info.internal_name),
        ("OriginalFilename", info.original_filename),
        ("ProductName", info.product_name),
        ("ProductVersion", info.version_text),
    ]
    .iter()
    .map(|(key, value)| {
        let text = utf16z(value);
        block(key, &text, (text.len() / 2) as u16, 1, &[])
    })
    .collect();
    let table = block("040904B0", &[], 0, 1, &strings);
    let string_file_info = block("StringFileInfo", &[], 0, 1, &[table]);
    let translation = block("Translation", &[0x09, 0x04, 0xB0, 0x04], 4, 0, &[]);
    let var_file_info = block("VarFileInfo", &[], 0, 1, &[translation]);
    block(
        "VS_VERSION_INFO",
        &fixed,
        fixed.len() as u16,
        0,
        &[string_file_info, var_file_info],
    )
}

/// One version block: length, value length, type, key, value, then its children, each starting on
/// a 4-byte boundary. The length counts everything but padding after the last child.
fn block(key: &str, value: &[u8], value_length: u16, kind: u16, children: &[Vec<u8>]) -> Vec<u8> {
    let mut out = vec![0, 0];
    out.extend_from_slice(&value_length.to_le_bytes());
    out.extend_from_slice(&kind.to_le_bytes());
    out.extend_from_slice(&utf16z(key));
    pad_to_dword(&mut out);
    out.extend_from_slice(value);
    for child in children {
        pad_to_dword(&mut out);
        out.extend_from_slice(child);
    }
    let length = out.len() as u16;
    out[..2].copy_from_slice(&length.to_le_bytes());
    out
}

fn utf16z(text: &str) -> Vec<u8> {
    text.encode_utf16()
        .chain(std::iter::once(0))
        .flat_map(u16::to_le_bytes)
        .collect()
}

fn pad_to_dword(out: &mut Vec<u8>) {
    while out.len() % 4 != 0 {
        out.push(0);
    }
}
