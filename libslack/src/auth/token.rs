use anyhow::{Result, bail};
use std::path::Path;

/// Scan LevelDB .ldb and .log files for xoxc-* token patterns.
/// Tokens are stored as plaintext strings in the binary LevelDB files.
pub fn extract_token(leveldb_dir: &Path) -> Result<String> {
    if !leveldb_dir.exists() {
        bail!(
            "Slack Local Storage directory not found: {}\n\
             Make sure Slack desktop is installed and you are signed in.",
            leveldb_dir.display()
        );
    }

    let mut tokens: Vec<String> = Vec::new();

    for entry in std::fs::read_dir(leveldb_dir)? {
        let entry = entry?;
        let path = entry.path();
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if ext != "ldb" && ext != "log" {
            continue;
        }

        let data = match std::fs::read(&path) {
            Ok(d) => d,
            Err(_) => continue,
        };

        extract_tokens_from_bytes(&data, &mut tokens);
    }

    tokens.sort_by(|a, b| b.len().cmp(&a.len()));
    tokens.dedup();

    match tokens.into_iter().next() {
        Some(token) => Ok(token),
        None => bail!(
            "No xoxc token found in Slack local storage.\n\
             Make sure you are signed into Slack desktop."
        ),
    }
}

fn extract_tokens_from_bytes(data: &[u8], tokens: &mut Vec<String>) {
    let needle = b"xoxc-";
    let mut i = 0;
    while i + needle.len() <= data.len() {
        if &data[i..i + needle.len()] == needle {
            let start = i;
            let mut end = i + needle.len();
            while end < data.len() && is_token_char(data[end]) {
                end += 1;
            }
            let candidate = &data[start..end];
            // Valid tokens are at least ~60 chars
            if candidate.len() >= 50 {
                if let Ok(s) = std::str::from_utf8(candidate) {
                    tokens.push(s.to_string());
                }
            }
            i = end;
        } else {
            i += 1;
        }
    }
}

fn is_token_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-'
}

#[cfg(test)]
mod tests {
    use super::{extract_tokens_from_bytes, extract_token};

    #[test]
    fn finds_longest_token_in_binary_blob() {
        let mut tokens = Vec::new();
        extract_tokens_from_bytes(
            b"junk xoxc-12345678901234567890123456789012345678901234567890 more xoxc-short",
            &mut tokens,
        );

        assert_eq!(tokens.len(), 1);
        assert!(tokens[0].starts_with("xoxc-1234567890"));
    }

    #[test]
    fn extract_token_reads_leveldb_like_files() {
        let base = std::env::temp_dir().join(format!(
            "libslack-token-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        std::fs::write(
            base.join("000001.log"),
            b"prefix xoxc-abcdefghijklmnopqrstuvwxyz0123456789abcdefghijklmnopqrstuvwxyz",
        )
        .unwrap();

        let token = extract_token(&base).unwrap();
        assert!(token.starts_with("xoxc-"));

        let _ = std::fs::remove_dir_all(&base);
    }
}
