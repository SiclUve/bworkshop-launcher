//! Empreintes SHA-256.

use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

pub fn to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0xf) as usize] as char);
    }
    s
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    to_hex(&Sha256::digest(bytes))
}

/// Ajoute le contenu d'un fichier à un calcul en cours (reprise de téléchargement).
pub fn update_from_file(hasher: &mut Sha256, path: &Path) -> io::Result<u64> {
    let mut file = File::open(path)?;
    let mut buf = vec![0u8; 64 * 1024];
    let mut total = 0;
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            return Ok(total);
        }
        hasher.update(&buf[..n]);
        total += n as u64;
    }
}

pub fn sha256_file(path: &Path) -> io::Result<String> {
    let mut hasher = Sha256::new();
    update_from_file(&mut hasher, path)?;
    Ok(to_hex(&hasher.finalize()))
}

/// Compare deux empreintes hexadécimales sans tenir compte de la casse.
pub fn same_digest(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empreintes_connues() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn fichier_modifie_detecte() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.bin");
        std::fs::write(&p, b"abc").unwrap();
        let attendu = sha256_hex(b"abc");
        assert!(same_digest(&sha256_file(&p).unwrap(), &attendu.to_uppercase()));
        std::fs::write(&p, b"abd").unwrap();
        assert!(!same_digest(&sha256_file(&p).unwrap(), &attendu));
    }
}
