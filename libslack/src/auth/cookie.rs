use aes::cipher::{BlockDecryptMut, KeyIvInit, block_padding::Pkcs7};
use anyhow::{Result, bail};
use std::path::Path;

type Aes128CbcDec = cbc::Decryptor<aes::Aes128>;

/// Extract and decrypt the `d` cookie (xoxd-*) from Slack's Cookies SQLite database.
///
/// Chromium on Linux encrypts cookies with AES-128-CBC using a key derived via PBKDF2.
/// The password is either "peanuts" (hardcoded default) or retrieved from the system keyring.
pub fn extract_cookie(cookies_path: &Path) -> Result<String> {
    if !cookies_path.exists() {
        bail!(
            "Slack Cookies database not found: {}\n\
             Make sure Slack desktop is installed and you are signed in.",
            cookies_path.display()
        );
    }

    let conn = rusqlite::Connection::open_with_flags(
        cookies_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;

    let encrypted_value: Vec<u8> = conn.query_row(
        "SELECT encrypted_value FROM cookies WHERE host_key LIKE '%slack.com' AND name = 'd' LIMIT 1",
        [],
        |row| row.get(0),
    )?;

    if encrypted_value.is_empty() {
        bail!("Cookie 'd' found but has empty encrypted value");
    }

    // Try decryption with "peanuts" first (Chromium default on Linux)
    if let Ok(value) = decrypt_cookie(&encrypted_value, b"peanuts") {
        return Ok(value);
    }

    // Try with empty string (some Electron builds)
    if let Ok(value) = decrypt_cookie(&encrypted_value, b"") {
        return Ok(value);
    }

    bail!(
        "Failed to decrypt Slack cookie. The cookie may be encrypted with the system keyring.\n\
         Try setting SLACK_COOKIE environment variable manually."
    );
}

fn decrypt_cookie(encrypted_value: &[u8], password: &[u8]) -> Result<String> {
    // Chromium encrypted cookies on Linux start with a version prefix
    let ciphertext = if encrypted_value.starts_with(b"v10") || encrypted_value.starts_with(b"v11") {
        &encrypted_value[3..]
    } else {
        encrypted_value
    };

    if ciphertext.is_empty() {
        bail!("Empty ciphertext after stripping version prefix");
    }

    // Derive key via PBKDF2-HMAC-SHA1
    let mut key = [0u8; 16];
    pbkdf2::pbkdf2_hmac::<sha1::Sha1>(password, b"saltysalt", 1, &mut key);

    // IV is 16 bytes of 0x20 (spaces)
    let iv = [0x20u8; 16];

    // Decrypt AES-128-CBC with PKCS7 padding
    let mut buf = ciphertext.to_vec();
    let decrypted = Aes128CbcDec::new(&key.into(), &iv.into())
        .decrypt_padded_mut::<Pkcs7>(&mut buf)
        .map_err(|e| anyhow::anyhow!("AES decryption failed: {}", e))?;

    // The first AES-CBC block (16 bytes) may decrypt to garbage if Chromium
    // uses a stored IV we don't have. Scan for the xoxd- token in the decrypted bytes.
    if let Some(pos) = decrypted.windows(5).position(|w| w == b"xoxd-") {
        let cookie_bytes = &decrypted[pos..];
        let value = String::from_utf8(cookie_bytes.to_vec())
            .map_err(|_| anyhow::anyhow!("Cookie value after xoxd- prefix is not valid UTF-8"))?;
        return Ok(value);
    }

    // Fallback: try the whole thing as UTF-8 (in case cookie format changes)
    let value = String::from_utf8_lossy(decrypted);
    if value.len() > 20 {
        Ok(value.into_owned())
    } else {
        bail!("Decrypted value doesn't contain a valid Slack cookie")
    }
}

#[cfg(test)]
mod tests {
    use super::decrypt_cookie;

    #[test]
    fn decrypt_cookie_rejects_gibberish() {
        let err = decrypt_cookie(b"not-a-real-cookie", b"peanuts").unwrap_err();
        assert!(
            err.to_string().contains("AES decryption failed")
                || err.to_string().contains("valid Slack cookie")
        );
    }
}
