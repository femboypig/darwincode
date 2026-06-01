use std::fs;
use anyhow::{Result, bail};
use sha2::{Sha256, Digest};
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce
};

/// Derive a secure, hardware-bound 256-bit symmetric key by hashing machine-id and user details.
pub fn derive_hardware_key() -> Result<[u8; 32]> {
    let mut hasher = Sha256::new();
    
    // Read local machine-id in a robust cross-platform manner
    let mut machine_id = String::new();

    #[cfg(target_os = "linux")]
    {
        if let Ok(id) = fs::read_to_string("/etc/machine-id") {
            machine_id = id.trim().to_owned();
        } else if let Ok(id) = fs::read_to_string("/var/lib/dbus/machine-id") {
            machine_id = id.trim().to_owned();
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(output) = std::process::Command::new("reg")
            .args(&["query", "HKLM\\SOFTWARE\\Microsoft\\Cryptography", "/v", "MachineGuid"])
            .output()
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if let Some(guid_line) = stdout.lines().find(|l| l.contains("MachineGuid")) {
                if let Some(guid) = guid_line.split_whitespace().last() {
                    machine_id = guid.trim().to_owned();
                }
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Ok(output) = std::process::Command::new("ioreg")
            .args(&["-rd1", "-c", "IOPlatformExpertDevice"])
            .output()
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if let Some(uuid_line) = stdout.lines().find(|l| l.contains("IOPlatformUUID")) {
                if let Some(uuid) = uuid_line.split('=').last() {
                    machine_id = uuid.replace('"', "").trim().to_owned();
                }
            }
        }
    }

    if machine_id.is_empty() {
        // Fallback to a stable default combined with home/user env
        machine_id = "stable-darwincode-fallback-key-998".to_owned();
    }
    
    let username = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "default_user".to_owned());
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| "/".to_owned());
    
    hasher.update(machine_id.as_bytes());
    hasher.update(username.as_bytes());
    hasher.update(home.as_bytes());
    
    let hash = hasher.finalize();
    let mut key = [0u8; 32];
    key.copy_from_slice(&hash);
    Ok(key)
}

/// Encrypt data using AES-256-GCM
pub fn encrypt_data(data: &[u8], key: &[u8; 32]) -> Result<Vec<u8>> {
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| anyhow::anyhow!("failed to create cipher: {}", e))?;
    
    // Generate a secure 96-bit (12-byte) nonce/IV
    let mut nonce_bytes = [0u8; 12];
    rand::fill(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    
    let ciphertext = cipher.encrypt(nonce, data)
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
    
    let plaintext = cipher.decrypt(nonce, ciphertext)
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
}
