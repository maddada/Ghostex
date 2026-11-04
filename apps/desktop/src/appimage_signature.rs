//! Checks an AppImage's embedded OpenPGP signature against a key the running app already holds.
//!
//! CDXC:Release 2026-10-28 WHY:
//! The AppImage updates itself (linux_updater.rs), so a downloaded image is run only after its
//! signature checks out against the Ghostex AppImage Signing key compiled into this build, never
//! the public key the download carries in its own `.sig_key` section (anyone can embed a key). The
//! format is `appimagetool --sign`'s, made by tooling/release-gpui/linux-appimage.sh and
//! appimage-signature.mjs: the signed text is the lowercase hex SHA-256 of the whole image with the
//! `.sha256_sig` and `.sig_key` sections read as zero bytes, and `.sha256_sig` holds an armored
//! detached v4 RSA/SHA-256 signature of it. Only that much OpenPGP is implemented, with the RSA
//! check done by `ring`, which the TLS stack already brings in.

use std::io::{Read, Seek, SeekFrom};

use base64::Engine as _;
use ring::digest;

pub(crate) const SIGNATURE_SECTION: &str = ".sha256_sig";
pub(crate) const KEY_SECTION: &str = ".sig_key";

/// The key every release AppImage is signed with (linux-appimage.sh `APPIMAGE_SIGNING_FINGERPRINT`).
pub(crate) const GHOSTEX_SIGNING_FINGERPRINT: &str = "093568F45FD8FFFED60AB4F94A5D4C4A8D2E45D3";
pub(crate) const GHOSTEX_SIGNING_PUBLIC_KEY: &str =
    include_str!("../../../tooling/packaging/appimage/ghostex-appimage-signing.asc");

/// A section's place in the file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SectionRange {
    pub(crate) offset: u64,
    pub(crate) size: u64,
}

/// Verifies `image` against the key in `trusted_armored_key` whose fingerprint is
/// `trusted_fingerprint`. Errors are plain sentences for the update dialog.
pub(crate) fn verify_appimage(
    image: &mut (impl Read + Seek),
    trusted_armored_key: &str,
    trusted_fingerprint: &str,
) -> Result<(), String> {
    let key = trusted_key(trusted_armored_key, trusted_fingerprint)?;
    let signature_range = find_elf_section(image, SIGNATURE_SECTION)?;
    let key_range = find_elf_section(image, KEY_SECTION)?;
    let armored_signature = read_c_string(image, signature_range)?;
    if armored_signature.trim().is_empty() {
        return Err("The download is not signed.".to_string());
    }
    let signature = parse_signature(&armored_signature)?;
    if let Some(issuer) = &signature.issuer_fingerprint
        && !issuer.eq_ignore_ascii_case(&key.fingerprint)
    {
        return Err("The download is signed by a different key than Ghostex's.".to_string());
    }
    let signed_text = signed_digest_hex(image, &[signature_range, key_range])?;
    verify_signature(&key, &signature, signed_text.as_bytes())
}

/// The lowercase hex SHA-256 of `image` with `zeroed` ranges read as zero bytes.
pub(crate) fn signed_digest_hex(
    image: &mut (impl Read + Seek),
    zeroed: &[SectionRange],
) -> Result<String, String> {
    image
        .seek(SeekFrom::Start(0))
        .map_err(|error| format!("Could not read the download: {error}"))?;
    let mut context = digest::Context::new(&digest::SHA256);
    let mut buffer = vec![0u8; 1 << 20];
    let mut position = 0u64;
    loop {
        let read = image
            .read(&mut buffer)
            .map_err(|error| format!("Could not read the download: {error}"))?;
        if read == 0 {
            break;
        }
        let chunk = &mut buffer[..read];
        for range in zeroed {
            let start = range.offset.max(position);
            let end = (range.offset + range.size).min(position + read as u64);
            if start < end {
                chunk[(start - position) as usize..(end - position) as usize].fill(0);
            }
        }
        context.update(chunk);
        position += read as u64;
    }
    Ok(hex::encode(context.finish().as_ref()))
}

/// Finds a named section in a 64-bit little-endian ELF (every AppImage runtime Ghostex ships).
pub(crate) fn find_elf_section(
    image: &mut (impl Read + Seek),
    name: &str,
) -> Result<SectionRange, String> {
    let not_appimage = || "The download is not an AppImage.".to_string();
    let mut header = [0u8; 64];
    image.seek(SeekFrom::Start(0)).map_err(|_| not_appimage())?;
    image.read_exact(&mut header).map_err(|_| not_appimage())?;
    if &header[..4] != b"\x7fELF" || header[4] != 2 || header[5] != 1 {
        return Err(not_appimage());
    }
    let section_headers = u64::from_le_bytes(header[0x28..0x30].try_into().unwrap_or_default());
    let entry_size = u16::from_le_bytes([header[0x3a], header[0x3b]]) as u64;
    let count = u16::from_le_bytes([header[0x3c], header[0x3d]]) as u64;
    let names_index = u16::from_le_bytes([header[0x3e], header[0x3f]]) as u64;
    if entry_size < 0x40 || count == 0 || names_index >= count || count > 4096 {
        return Err(not_appimage());
    }
    let mut read_section = |index: u64| -> Result<(u32, SectionRange), String> {
        let mut entry = [0u8; 0x40];
        image
            .seek(SeekFrom::Start(section_headers + index * entry_size))
            .map_err(|_| not_appimage())?;
        image.read_exact(&mut entry).map_err(|_| not_appimage())?;
        let name = u32::from_le_bytes(entry[0..4].try_into().unwrap_or_default());
        let offset = u64::from_le_bytes(entry[0x18..0x20].try_into().unwrap_or_default());
        let size = u64::from_le_bytes(entry[0x20..0x28].try_into().unwrap_or_default());
        Ok((name, SectionRange { offset, size }))
    };
    let (_, names) = read_section(names_index)?;
    let sections = (0..count)
        .map(&mut read_section)
        .collect::<Result<Vec<_>, _>>()?;
    if names.size > 1 << 20 {
        return Err(not_appimage());
    }
    let mut table = vec![0u8; names.size as usize];
    image
        .seek(SeekFrom::Start(names.offset))
        .map_err(|_| not_appimage())?;
    image.read_exact(&mut table).map_err(|_| not_appimage())?;
    sections
        .into_iter()
        .find(|(name_offset, _)| {
            let start = *name_offset as usize;
            table
                .get(start..)
                .and_then(|rest| rest.split(|byte| *byte == 0).next())
                .is_some_and(|found| found == name.as_bytes())
        })
        .map(|(_, range)| range)
        .ok_or_else(|| format!("The download has no {name} section, so it is not signed."))
}

fn read_c_string(image: &mut (impl Read + Seek), range: SectionRange) -> Result<String, String> {
    if range.size > 1 << 20 {
        return Err("The download's signature section is too large.".to_string());
    }
    let mut bytes = vec![0u8; range.size as usize];
    image
        .seek(SeekFrom::Start(range.offset))
        .and_then(|_| image.read_exact(&mut bytes))
        .map_err(|error| format!("Could not read the download: {error}"))?;
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    String::from_utf8(bytes[..end].to_vec())
        .map_err(|_| "The download's signature is unreadable.".to_string())
}

struct RsaKey {
    fingerprint: String,
    modulus: Vec<u8>,
    exponent: Vec<u8>,
}

struct Signature {
    signature_type: u8,
    hash_algorithm: u8,
    hashed_part: Vec<u8>,
    issuer_fingerprint: Option<String>,
    value: Vec<u8>,
}

/// The RSA key (primary or subkey) in `armored` whose fingerprint is `fingerprint`.
fn trusted_key(armored: &str, fingerprint: &str) -> Result<RsaKey, String> {
    let packets = parse_packets(&dearmor(armored)?)?;
    packets
        .iter()
        .filter(|(tag, _)| *tag == 6 || *tag == 14)
        .filter_map(|(_, body)| parse_rsa_key(body))
        .find(|key| key.fingerprint.eq_ignore_ascii_case(fingerprint))
        .ok_or_else(|| "This build's signing key is missing or unreadable.".to_string())
}

fn parse_rsa_key(body: &[u8]) -> Option<RsaKey> {
    // v4: version, creation time (4), algorithm, then the algorithm's MPIs.
    if body.first() != Some(&4) || !matches!(body.get(5), Some(1 | 3)) {
        return None;
    }
    let mut cursor = 6;
    let modulus = read_mpi(body, &mut cursor)?;
    let exponent = read_mpi(body, &mut cursor)?;
    let mut hashed = vec![0x99, (body.len() >> 8) as u8, body.len() as u8];
    hashed.extend_from_slice(body);
    let fingerprint = digest::digest(&digest::SHA1_FOR_LEGACY_USE_ONLY, &hashed);
    Some(RsaKey {
        fingerprint: hex::encode_upper(fingerprint.as_ref()),
        modulus,
        exponent,
    })
}

fn parse_signature(armored: &str) -> Result<Signature, String> {
    let unreadable = || "The download's signature is unreadable.".to_string();
    let packets = parse_packets(&dearmor(armored)?)?;
    let body = packets
        .into_iter()
        .find_map(|(tag, body)| (tag == 2).then_some(body))
        .ok_or_else(unreadable)?;
    if body.len() < 6 || body[0] != 4 {
        return Err(unreadable());
    }
    let hashed_length = u16::from_be_bytes([body[4], body[5]]) as usize;
    let hashed_end = 6 + hashed_length;
    let hashed_part = body.get(..hashed_end).ok_or_else(unreadable)?.to_vec();
    let unhashed_length = u16::from_be_bytes([
        *body.get(hashed_end).ok_or_else(unreadable)?,
        *body.get(hashed_end + 1).ok_or_else(unreadable)?,
    ]) as usize;
    let mut cursor = hashed_end + 2 + unhashed_length + 2;
    if body[3] != 8 {
        return Err("The download's signature uses a hash Ghostex does not accept.".to_string());
    }
    if !matches!(body[2], 1 | 3) {
        return Err("The download's signature is not an RSA signature.".to_string());
    }
    let value = read_mpi(&body, &mut cursor).ok_or_else(unreadable)?;
    Ok(Signature {
        signature_type: body[1],
        hash_algorithm: body[3],
        issuer_fingerprint: issuer_fingerprint(&hashed_part[6..]),
        hashed_part,
        value,
    })
}

/// The issuer fingerprint subpacket (type 33, v4 key) among the hashed subpackets.
fn issuer_fingerprint(mut subpackets: &[u8]) -> Option<String> {
    while let Some(&first) = subpackets.first() {
        let (length, header) = match first {
            0..=191 => (first as usize, 1),
            192..=254 => (
                ((first as usize - 192) << 8) + *subpackets.get(1)? as usize + 192,
                2,
            ),
            255 => (
                u32::from_be_bytes(subpackets.get(1..5)?.try_into().ok()?) as usize,
                5,
            ),
        };
        let packet = subpackets.get(header..header + length)?;
        if packet.first().map(|kind| kind & 0x7f) == Some(33) && packet.get(1) == Some(&4) {
            return Some(hex::encode_upper(packet.get(2..22)?));
        }
        subpackets = &subpackets[header + length..];
    }
    None
}

fn verify_signature(key: &RsaKey, signature: &Signature, text: &[u8]) -> Result<(), String> {
    if signature.hash_algorithm != 8 || !matches!(signature.signature_type, 0 | 1) {
        return Err("The download's signature is not a file signature.".to_string());
    }
    // A text signature signs the text with CRLF line ends; the signed digest has no line end.
    let mut message = if signature.signature_type == 1 {
        canonical_text(text)
    } else {
        text.to_vec()
    };
    message.extend_from_slice(&signature.hashed_part);
    message.extend_from_slice(&[0x04, 0xff]);
    message.extend_from_slice(&(signature.hashed_part.len() as u32).to_be_bytes());
    // ring wants the signature as long as the modulus; an MPI drops leading zero bytes.
    let mut value = vec![0u8; key.modulus.len().saturating_sub(signature.value.len())];
    value.extend_from_slice(&signature.value);
    ring::signature::RsaPublicKeyComponents {
        n: key.modulus.as_slice(),
        e: key.exponent.as_slice(),
    }
    .verify(&ring::signature::RSA_PKCS1_2048_8192_SHA256, &message, &value)
    .map_err(|_| {
        "The download's signature does not match. It may be damaged or not from Ghostex, so it was not installed."
            .to_string()
    })
}

fn canonical_text(text: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    for (index, byte) in text.iter().enumerate() {
        if *byte == b'\n' && (index == 0 || text[index - 1] != b'\r') {
            out.push(b'\r');
        }
        out.push(*byte);
    }
    out
}

fn dearmor(armored: &str) -> Result<Vec<u8>, String> {
    let unreadable = || "An OpenPGP block is unreadable.".to_string();
    let mut lines = armored.lines().map(str::trim);
    lines
        .by_ref()
        .find(|line| line.starts_with("-----BEGIN PGP "))
        .ok_or_else(unreadable)?;
    // Armor headers ("Version: …") end at the first blank line, if there are any.
    let mut body = String::new();
    let mut in_headers = true;
    for line in lines {
        if line.starts_with("-----END PGP ") {
            break;
        }
        if in_headers {
            if line.is_empty() {
                in_headers = false;
                continue;
            }
            if line.contains(": ") {
                continue;
            }
            in_headers = false;
        }
        if line.starts_with('=') && line.len() == 5 {
            continue; // the CRC-24 line
        }
        body.push_str(line);
    }
    base64::engine::general_purpose::STANDARD
        .decode(body)
        .map_err(|_| unreadable())
}

/// (tag, body) for every packet, old and new header formats.
fn parse_packets(mut data: &[u8]) -> Result<Vec<(u8, Vec<u8>)>, String> {
    let unreadable = || "An OpenPGP block is unreadable.".to_string();
    let mut packets = Vec::new();
    while let Some(&first) = data.first() {
        if first & 0x80 == 0 {
            return Err(unreadable());
        }
        let (tag, length, header) = if first & 0x40 != 0 {
            let tag = first & 0x3f;
            let octet = *data.get(1).ok_or_else(unreadable)? as usize;
            match octet {
                0..=191 => (tag, octet, 2),
                192..=223 => (
                    tag,
                    ((octet - 192) << 8) + *data.get(2).ok_or_else(unreadable)? as usize + 192,
                    3,
                ),
                255 => (
                    tag,
                    u32::from_be_bytes(
                        data.get(2..6)
                            .ok_or_else(unreadable)?
                            .try_into()
                            .map_err(|_| unreadable())?,
                    ) as usize,
                    6,
                ),
                _ => return Err(unreadable()),
            }
        } else {
            let tag = (first >> 2) & 0x0f;
            match first & 0x03 {
                0 => (tag, *data.get(1).ok_or_else(unreadable)? as usize, 2),
                1 => (
                    tag,
                    u16::from_be_bytes([
                        *data.get(1).ok_or_else(unreadable)?,
                        *data.get(2).ok_or_else(unreadable)?,
                    ]) as usize,
                    3,
                ),
                2 => (
                    tag,
                    u32::from_be_bytes(
                        data.get(1..5)
                            .ok_or_else(unreadable)?
                            .try_into()
                            .map_err(|_| unreadable())?,
                    ) as usize,
                    5,
                ),
                _ => return Err(unreadable()),
            }
        };
        let body = data.get(header..header + length).ok_or_else(unreadable)?;
        packets.push((tag, body.to_vec()));
        data = &data[header + length..];
    }
    Ok(packets)
}

fn read_mpi(body: &[u8], cursor: &mut usize) -> Option<Vec<u8>> {
    let bits = u16::from_be_bytes([*body.get(*cursor)?, *body.get(*cursor + 1)?]) as usize;
    let length = bits.div_ceil(8);
    let value = body.get(*cursor + 2..*cursor + 2 + length)?.to_vec();
    *cursor += 2 + length;
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    // A tiny ELF with both sections, signed by a throwaway test key the way the release signs
    // (gpg --digest-algo SHA256 --armor --textmode --detach-sign of the signed digest).
    // Made by testdata/appimage-signature/make-fixture.sh.
    const FIXTURE: &[u8] = include_bytes!("../testdata/appimage-signature/signed.elf");
    const TEST_KEY: &str = include_str!("../testdata/appimage-signature/test-key.asc");
    const TEST_FINGERPRINT: &str = include_str!("../testdata/appimage-signature/test-key.fpr");

    #[test]
    fn a_signed_image_verifies_against_the_trusted_key() {
        verify_appimage(&mut Cursor::new(FIXTURE), TEST_KEY, TEST_FINGERPRINT.trim()).unwrap();
    }

    #[test]
    fn a_changed_byte_outside_the_signature_is_refused() {
        let mut tampered = FIXTURE.to_vec();
        let last = tampered.len() - 1;
        tampered[last] ^= 0x01;
        let error = verify_appimage(
            &mut Cursor::new(tampered),
            TEST_KEY,
            TEST_FINGERPRINT.trim(),
        )
        .unwrap_err();
        assert!(error.contains("does not match"), "{error}");
    }

    #[test]
    fn the_key_inside_the_download_is_never_trusted() {
        // Ghostex's real key did not make this signature, whatever key the image carries.
        let error = verify_appimage(
            &mut Cursor::new(FIXTURE),
            GHOSTEX_SIGNING_PUBLIC_KEY,
            GHOSTEX_SIGNING_FINGERPRINT,
        )
        .unwrap_err();
        assert!(
            error.contains("different key") || error.contains("does not match"),
            "{error}"
        );
    }

    #[test]
    fn an_unsigned_image_is_refused() {
        let mut unsigned = FIXTURE.to_vec();
        let range = find_elf_section(&mut Cursor::new(&unsigned), SIGNATURE_SECTION).unwrap();
        unsigned[range.offset as usize..(range.offset + range.size) as usize].fill(0);
        let error = verify_appimage(
            &mut Cursor::new(unsigned),
            TEST_KEY,
            TEST_FINGERPRINT.trim(),
        )
        .unwrap_err();
        assert_eq!(error, "The download is not signed.");
    }

    #[test]
    fn the_bundled_key_is_the_release_signing_key() {
        let key = trusted_key(GHOSTEX_SIGNING_PUBLIC_KEY, GHOSTEX_SIGNING_FINGERPRINT).unwrap();
        assert!(key.modulus.len() >= 512, "RSA 4096");
    }

    #[test]
    fn the_digest_reads_the_sections_as_zeros() {
        let mut image = Cursor::new(FIXTURE);
        let ranges = [
            find_elf_section(&mut image, SIGNATURE_SECTION).unwrap(),
            find_elf_section(&mut image, KEY_SECTION).unwrap(),
        ];
        let expected = include_str!("../testdata/appimage-signature/signed-digest.txt");
        assert_eq!(
            signed_digest_hex(&mut image, &ranges).unwrap(),
            expected.trim()
        );
    }

    /// A real release AppImage against the key compiled into this build:
    /// `GHOSTEX_VERIFY_APPIMAGE=/path/to/ghostex-X-linux-x64.AppImage cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn a_release_appimage_verifies_against_the_bundled_key() {
        let path = std::env::var("GHOSTEX_VERIFY_APPIMAGE").expect("GHOSTEX_VERIFY_APPIMAGE");
        let mut image = std::fs::File::open(path).unwrap();
        verify_appimage(
            &mut image,
            GHOSTEX_SIGNING_PUBLIC_KEY,
            GHOSTEX_SIGNING_FINGERPRINT,
        )
        .unwrap();
    }
}
