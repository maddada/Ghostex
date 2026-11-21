//! Release packaging for the Windows installer, run by tooling/release-gpui/windows.ps1.
//!
//! `store <Setup.exe>` rewrites the package Velopack embeds in its unsigned Setup.exe with uncompressed entries.
//! `bundle <launcher.exe> <Setup.exe> <installer.exe>` appends that Setup.exe, LZMA-compressed, to the Ghostex installer launcher (src/bin/ghostex_windows_installer.rs).
//!
//! CDXC:Release 2026-10-23 DECISION:
//! The user chose LZMA compression for the Windows installer: it was the largest download on the release page because Velopack packs every file with zip's Deflate, which left the 179 MB app executable at 53 MB, while LZMA (as in the macOS DMG) holds the whole app in about 54 MB instead of 87. Velopack's updater and Setup read only Deflate or uncompressed zip entries, so its Setup.exe carries the package uncompressed and the installer launcher carries that Setup.exe as one xz stream, unpacking it before it runs. The update packages (.nupkg) stay Deflate. Velopack copies the installed package into its packages folder, so a fresh install keeps the uncompressed package (about 250 MB) there until its first update replaces it.
//! SEE-ALSO: src/windows_installer_payload.rs (the trailer), src/bin/ghostex_windows_installer.rs (unpacking), tooling/release-gpui/windows.ps1 (pack and bundle phases).
#![cfg_attr(not(target_os = "windows"), allow(dead_code))]

#[path = "../windows_installer_payload.rs"]
mod windows_installer_payload;

#[cfg(target_os = "windows")]
fn main() -> std::process::ExitCode {
    let arguments: Vec<std::path::PathBuf> = std::env::args_os()
        .skip(1)
        .map(std::path::PathBuf::from)
        .collect();
    let result = match arguments.as_slice() {
        [command, setup] if command.as_os_str() == "store" => windows::store(setup),
        [command, launcher, setup, installer] if command.as_os_str() == "bundle" => {
            windows::bundle(launcher, setup, installer)
        }
        _ => Err(
            "usage: ghostex-windows-installer-pack store <Setup.exe> | bundle <launcher.exe> <Setup.exe> <installer.exe>"
                .to_string(),
        ),
    };
    match result {
        Ok(message) => {
            println!("{message}");
            std::process::ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn main() {
    eprintln!(
        "ghostex-windows-installer-pack packages the Windows installer and runs on Windows only."
    );
    std::process::exit(2);
}

#[cfg(target_os = "windows")]
mod windows {
    use super::windows_installer_payload::{LENGTH_LEN, MAGIC};
    use sha2::{Digest, Sha256};
    use std::{
        fs::{self, File},
        io::{self, Cursor, Read, Seek, Write},
        path::Path,
    };

    /// The 32 bytes after the package offset and length in Velopack's Setup.exe (`BUNDLE_PLACEHOLDER` in velopack's src/bins/src/setup.rs).
    const BUNDLE_SIGNATURE: [u8; 32] = [
        0x94, 0xf0, 0xb1, 0x7b, 0x68, 0x93, 0xe0, 0x29, 0x37, 0xeb, 0x34, 0xef, 0x53, 0xaa, 0xe7,
        0xd4, 0x2b, 0x54, 0xf5, 0x70, 0x7e, 0xf5, 0xd6, 0xf5, 0x78, 0x54, 0x98, 0x3e, 0x5e, 0x94,
        0xed, 0x7d,
    ];

    pub(super) fn store(setup: &Path) -> Result<String, String> {
        let bytes =
            fs::read(setup).map_err(|error| format!("Reading {}: {error}", setup.display()))?;
        let signature = bytes
            .windows(BUNDLE_SIGNATURE.len())
            .position(|window| window == BUNDLE_SIGNATURE)
            .filter(|position| *position >= 16)
            .ok_or("Setup.exe has no Velopack bundle header")?;
        let header = signature - 16;
        let offset = u64::from_le_bytes(bytes[header..header + 8].try_into().unwrap()) as usize;
        let length =
            u64::from_le_bytes(bytes[header + 8..header + 16].try_into().unwrap()) as usize;
        // A signature appended after the package would be broken by the rewrite, so only an unsigned Setup.exe is accepted.
        if offset <= signature || length == 0 || offset.checked_add(length) != Some(bytes.len()) {
            return Err(format!(
                "Setup.exe's package (offset {offset}, length {length}) does not end the unsigned {}-byte file",
                bytes.len()
            ));
        }
        let mut original = zip::ZipArchive::new(Cursor::new(&bytes[offset..]))
            .map_err(|error| format!("Reading Setup.exe's package: {error}"))?;
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for index in 0..original.len() {
            let mut entry = original
                .by_index(index)
                .map_err(|error| error.to_string())?;
            let mut options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored)
                .large_file(entry.size() >= u64::from(u32::MAX));
            if let Some(modified) = entry.last_modified() {
                options = options.last_modified_time(modified);
            }
            // zip 9: `name()` reports a name that is not valid UTF-8 as an error.
            let name = entry
                .name()
                .map_err(|error| error.to_string())?
                .into_owned();
            if entry.is_dir() {
                writer.add_directory(name.as_str(), options)
            } else {
                writer.start_file(name.as_str(), options)
            }
            .map_err(|error| format!("Writing {name}: {error}"))?;
            if !entry.is_dir() {
                io::copy(&mut entry, &mut writer)
                    .map_err(|error| format!("Copying {name}: {error}"))?;
            }
        }
        let stored = writer
            .finish()
            .map_err(|error| format!("Finishing the package: {error}"))?
            .into_inner();
        // The rewritten package must hold the same entries with the same contents.
        let mut check =
            zip::ZipArchive::new(Cursor::new(&stored)).map_err(|error| error.to_string())?;
        if check.len() != original.len() {
            return Err("The uncompressed package lost entries".to_string());
        }
        for index in 0..original.len() {
            let before = original
                .by_index_raw(index)
                .map_err(|error| error.to_string())?;
            let after = check
                .by_index_raw(index)
                .map_err(|error| error.to_string())?;
            if before.name_raw() != after.name_raw()
                || before.crc32() != after.crc32()
                || before.size() != after.size()
            {
                return Err(format!(
                    "{} changed while being stored",
                    String::from_utf8_lossy(before.name_raw())
                ));
            }
        }
        let mut rewritten = bytes[..offset].to_vec();
        rewritten[header + 8..header + 16].copy_from_slice(&(stored.len() as u64).to_le_bytes());
        rewritten.extend_from_slice(&stored);
        let temporary = setup.with_extension("stored.tmp");
        fs::write(&temporary, &rewritten)
            .and_then(|()| fs::rename(&temporary, setup))
            .map_err(|error| format!("Writing {}: {error}", setup.display()))?;
        Ok(format!(
            "Stored the {} package entries of {} uncompressed: {length} -> {} bytes",
            original.len(),
            setup.display(),
            stored.len()
        ))
    }

    pub(super) fn bundle(
        launcher: &Path,
        setup: &Path,
        installer: &Path,
    ) -> Result<String, String> {
        let mut output = File::create(installer)
            .map_err(|error| format!("Creating {}: {error}", installer.display()))?;
        let launcher_len = io::copy(
            &mut File::open(launcher)
                .map_err(|error| format!("Opening {}: {error}", launcher.display()))?,
            &mut output,
        )
        .map_err(|error| format!("Copying the launcher: {error}"))?;
        let threads = std::thread::available_parallelism().map_or(1, |count| count.get() as u32);
        let stream = liblzma::stream::MtStreamBuilder::new()
            .preset(9 | liblzma::stream::PRESET_EXTREME)
            .threads(threads)
            .check(liblzma::stream::Check::Crc64)
            .encoder()
            .map_err(|error| format!("Starting the LZMA encoder: {error}"))?;
        let mut encoder = liblzma::write::XzEncoder::new_stream(&mut output, stream);
        let mut setup_hash = Sha256::new();
        let mut source =
            File::open(setup).map_err(|error| format!("Opening {}: {error}", setup.display()))?;
        let mut buffer = vec![0_u8; 1 << 20];
        loop {
            let count = source
                .read(&mut buffer)
                .map_err(|error| error.to_string())?;
            if count == 0 {
                break;
            }
            setup_hash.update(&buffer[..count]);
            encoder
                .write_all(&buffer[..count])
                .map_err(|error| format!("Compressing: {error}"))?;
        }
        encoder
            .finish()
            .map_err(|error| format!("Compressing: {error}"))?;
        let payload_len = output
            .stream_position()
            .map_err(|error| error.to_string())?
            - launcher_len;
        output.write_all(MAGIC).map_err(|error| error.to_string())?;
        output
            .write_all(&payload_len.to_le_bytes()[..LENGTH_LEN])
            .map_err(|error| error.to_string())?;
        output.sync_all().map_err(|error| error.to_string())?;
        drop(output);

        // Unpack what was written and compare it with the Setup.exe, as the launcher will.
        let mut written = File::open(installer).map_err(|error| error.to_string())?;
        written
            .seek(io::SeekFrom::Start(launcher_len))
            .map_err(|error| error.to_string())?;
        let mut decoder = liblzma::read::XzDecoder::new(written.take(payload_len));
        let mut unpacked_hash = Sha256::new();
        loop {
            let count = decoder
                .read(&mut buffer)
                .map_err(|error| format!("Unpacking the written payload: {error}"))?;
            if count == 0 {
                break;
            }
            unpacked_hash.update(&buffer[..count]);
        }
        if unpacked_hash.finalize() != setup_hash.finalize() {
            return Err("The compressed Setup.exe does not unpack to the original".to_string());
        }
        Ok(format!(
            "Bundled {} into {}: {} bytes of LZMA after the {launcher_len}-byte launcher",
            setup.display(),
            installer.display(),
            payload_len
        ))
    }
}
