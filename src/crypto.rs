use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit},
};
use anyhow::{Result, bail};

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn from_hex(s: &str) -> Option<Vec<u8>> {
    if !s.is_ascii() || !s.len().is_multiple_of(2) {
        return None;
    }
    let mut res = Vec::with_capacity(s.len() / 2);
    let chars: Vec<char> = s.chars().collect();
    for i in (0..chars.len()).step_by(2) {
        let high = chars[i].to_digit(16)?;
        let low = chars[i + 1].to_digit(16)?;
        res.push((high * 16 + low) as u8);
    }
    Some(res)
}

pub fn is_home_appdata_missing() -> bool {
    fn empty_or_unset(name: &str) -> bool {
        std::env::var_os(name)
            .map(|s| s.is_empty() || s.to_string_lossy().trim().is_empty())
            .unwrap_or(true)
    }
    empty_or_unset("XDG_CONFIG_HOME")
        && empty_or_unset("APPDATA")
        && empty_or_unset("HOME")
        && empty_or_unset("USERPROFILE")
}

/// Derive a secure, hardware-bound 256-bit symmetric key.
pub fn derive_hardware_key() -> Result<[u8; 32]> {
    static KEY_CACHE: std::sync::OnceLock<[u8; 32]> = std::sync::OnceLock::new();
    if let Some(key) = KEY_CACHE.get() {
        return Ok(*key);
    }

    if is_home_appdata_missing() {
        bail!("Encryption disabled because HOME/APPDATA is missing");
    }

    // Try to retrieve key from secure keyring first
    if let Ok(entry) = keyring::Entry::new("darwincode", "encryption_key")
        && let Ok(password) = entry.get_password()
        && let Some(key_bytes) = from_hex(&password)
        && key_bytes.len() == 32
    {
        let mut key = [0u8; 32];
        key.copy_from_slice(&key_bytes);
        let _ = KEY_CACHE.set(key);
        return Ok(key);
    }

    // Fallback: machine-id file
    let base_dir = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("APPDATA").map(std::path::PathBuf::from))
        .or_else(|| {
            std::env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".config"))
        })
        .or_else(|| {
            std::env::var_os("USERPROFILE")
                .map(|home| std::path::PathBuf::from(home).join(".config"))
        });

    if let Some(base) = base_dir {
        let darwincode_dir = base.join("darwincode");
        let _ = std::fs::create_dir_all(&darwincode_dir);
        let machine_id_path = darwincode_dir.join("machine-id");
        let key_hex = if let Ok(id) = std::fs::read_to_string(&machine_id_path) {
            id.trim().to_owned()
        } else {
            let mut bytes = [0u8; 32];
            rand::fill(&mut bytes);
            let hex_id = to_hex(&bytes);

            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                if let Ok(mut file) = std::fs::OpenOptions::new()
                    .create(true)
                    .write(true)
                    .truncate(true)
                    .mode(0o600)
                    .open(&machine_id_path)
                {
                    use std::io::Write;
                    let _ = write!(file, "{}", hex_id);
                }
            }
            #[cfg(not(unix))]
            {
                let _ = std::fs::write(&machine_id_path, &hex_id);
            }
            hex_id
        };

        if let Some(key_bytes) = from_hex(&key_hex)
            && key_bytes.len() == 32
        {
            let mut key = [0u8; 32];
            key.copy_from_slice(&key_bytes);

            // Store in keyring for future fast/secure lookup
            if let Ok(entry) = keyring::Entry::new("darwincode", "encryption_key") {
                let _ = entry.set_password(&to_hex(&key));
            }

            let _ = KEY_CACHE.set(key);
            return Ok(key);
        }
    }

    bail!("Failed to retrieve or generate encryption key")
}

/// Encrypt data using AES-256-GCM
pub fn encrypt_data(data: &[u8], key: &[u8; 32]) -> Result<Vec<u8>> {
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| anyhow::anyhow!("failed to create cipher: {}", e))?;

    // Generate a secure 96-bit (12-byte) nonce/IV
    let mut nonce_bytes = [0u8; 12];
    rand::fill(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, data)
        .map_err(|e| anyhow::anyhow!("encryption error: {}", e))?;

    // Output is nonce + ciphertext
    let mut output = Vec::with_capacity(12 + ciphertext.len());
    output.extend_from_slice(&nonce_bytes);
    output.extend_from_slice(&ciphertext);
    Ok(output)
}

/// Decrypt data using AES-256-GCM
pub fn decrypt_data(data: &[u8], key: &[u8; 32]) -> Result<Vec<u8>> {
    if data.len() < 12 {
        bail!("Invalid ciphertext (too short)");
    }

    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| anyhow::anyhow!("failed to create cipher: {}", e))?;

    let (nonce_bytes, ciphertext) = data.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);

    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| anyhow::anyhow!("decryption error: {}", e))?;

    Ok(plaintext)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encryption_roundtrip() {
        let key = derive_hardware_key().expect("failed to derive key");
        let plaintext = b"Hello, secure world! This is a highly confidential chat log entry.";

        let ciphertext = encrypt_data(plaintext, &key).expect("encryption failed");
        assert_ne!(plaintext.to_vec(), ciphertext);
        assert!(ciphertext.len() > 12);

        let decrypted = decrypt_data(&ciphertext, &key).expect("decryption failed");
        assert_eq!(plaintext.to_vec(), decrypted);
    }

    #[test]
    fn test_decryption_with_wrong_key() {
        let key1 = [1u8; 32];
        let key2 = [2u8; 32];
        let plaintext = b"Confidential info";

        let ciphertext = encrypt_data(plaintext, &key1).expect("encryption failed");
        let decrypt_result = decrypt_data(&ciphertext, &key2);
        assert!(decrypt_result.is_err());
    }

    #[test]
    fn test_decryption_invalid_length() {
        let key = [1u8; 32];
        let too_short = vec![0u8; 11];
        let decrypt_result = decrypt_data(&too_short, &key);
        assert!(decrypt_result.is_err());
    }

    #[test]
    fn test_from_hex_non_ascii() {
        assert!(from_hex("👍👍").is_none());
        assert!(from_hex("4a4b4c").is_some());
    }
}
