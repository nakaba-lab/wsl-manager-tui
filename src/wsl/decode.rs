//! Decoding of `wsl.exe` output.
//!
//! The wsl CLI emits its own messages as UTF-16LE (usually with a BOM). By
//! contrast, in-distro command output (`wsl -d <name> -- ...`) is the Linux
//! side's UTF-8 and must be decoded with [`decode_utf8`].

/// Decode `wsl.exe` meta output. Handles an optional UTF-16LE BOM. If the bytes
/// do not look like UTF-16 (no BOM and no interior NUL bytes) they are treated
/// as UTF-8, so the function is robust across environments.
pub fn decode_wsl_output(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        return decode_utf16le(rest);
    }
    // Heuristic: UTF-16LE encodes every ASCII character with a NUL high byte.
    // Scan the *whole* buffer (not just the first few units): localized wsl
    // output can begin with non-ASCII prose — e.g. `wsl --list --online` on a
    // Japanese locale starts with "インストール…" whose high bytes are 0x30 —
    // but it always contains ASCII later (distro ids, "NAME", spaces, newlines),
    // so a NUL high byte appears somewhere.
    let looks_utf16 = bytes.len() >= 2 && bytes.iter().skip(1).step_by(2).any(|&b| b == 0);
    if looks_utf16 {
        decode_utf16le(bytes)
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    }
}

/// Decode raw UTF-8 bytes (used for in-distro command output).
pub fn decode_utf8(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn decode_utf16le(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    String::from_utf16_lossy(&units)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utf16le_with_bom(s: &str) -> Vec<u8> {
        let mut v = vec![0xFF, 0xFE];
        for u in s.encode_utf16() {
            v.extend_from_slice(&u.to_le_bytes());
        }
        v
    }

    #[test]
    fn decodes_utf16le_with_bom() {
        let bytes = utf16le_with_bom("  NAME  STATE\r\n* Debian  Running  2\r\n");
        let s = decode_wsl_output(&bytes);
        assert!(s.contains("Debian"));
        assert!(s.contains("NAME"));
    }

    #[test]
    fn decodes_utf16le_without_bom() {
        let mut bytes = Vec::new();
        for u in "Ubuntu".encode_utf16() {
            bytes.extend_from_slice(&u.to_le_bytes());
        }
        assert_eq!(decode_wsl_output(&bytes), "Ubuntu");
    }

    #[test]
    fn falls_back_to_utf8() {
        assert_eq!(decode_wsl_output(b"plain ascii"), "plain ascii");
    }

    #[test]
    fn decodes_localized_state() {
        // Japanese (localized) state strings must still decode cleanly.
        let bytes = utf16le_with_bom("  名前  状態  バージョン\r\n* Debian  実行中  2\r\n");
        let s = decode_wsl_output(&bytes);
        assert!(s.contains("実行中"));
        assert!(s.contains("Debian"));
    }

    /// Build BOM-less UTF-16LE bytes from `s` (matches real `wsl --list --online`).
    fn utf16le_no_bom(s: &str) -> Vec<u8> {
        let mut v = Vec::new();
        for u in s.encode_utf16() {
            v.extend_from_slice(&u.to_le_bytes());
        }
        v
    }

    #[test]
    fn decodes_bomless_utf16_starting_with_non_ascii() {
        // Regression: `wsl --list --online` on a Japanese locale emits BOM-less
        // UTF-16LE whose first characters are localized prose (high byte 0x30,
        // not 0x00). The decoder must still recognise it as UTF-16 rather than
        // fall back to UTF-8 mojibake (which made the installable list empty).
        let bytes = utf16le_no_bom(
            "インストールできる有効なディストリビューションの一覧を次に示します。\r\n\
\r\n\
NAME            FRIENDLY NAME\r\n\
Ubuntu          Ubuntu\r\n\
Debian          Debian GNU/Linux\r\n",
        );
        let decoded = decode_wsl_output(&bytes);
        assert!(
            decoded.contains("Ubuntu"),
            "expected Ubuntu, got: {decoded:?}"
        );
        assert!(decoded.contains("FRIENDLY NAME"));
        assert!(
            !decoded.contains('\u{FFFD}'),
            "should be decoded as UTF-16, not UTF-8 mojibake: {decoded:?}"
        );
    }
}
