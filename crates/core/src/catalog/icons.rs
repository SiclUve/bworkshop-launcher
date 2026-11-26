//! Icônes des outils (champ `iconUrl` du catalogue).
//!
//! - Téléchargées seulement depuis les domaines autorisés, en HTTPS (comme le reste).
//! - 512 Ko au plus ; PNG, WebP, JPEG ou SVG, reconnus par leur contenu.
//! - Affichées seulement dans une balise `<img>` : un éventuel script dans un SVG
//!   n'y est jamais exécuté (règle du navigateur), et la CSP des pages l'interdit aussi.
//! - Gardées dans `%LOCALAPPDATA%\BworkshopLauncher\cache\icons` : affichées hors ligne.
//! - Remises à l'interface en adresse `data:` (autorisée par la CSP des pages locales).

use super::{HttpGet, HttpResponse};
use crate::security::hash;
use crate::storage;
use std::fs;
use std::path::PathBuf;

pub const MAX_ICON_BYTES: usize = 512 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageKind {
    Png,
    Webp,
    Jpeg,
    Svg,
}

impl ImageKind {
    pub fn mime(self) -> &'static str {
        match self {
            ImageKind::Png => "image/png",
            ImageKind::Webp => "image/webp",
            ImageKind::Jpeg => "image/jpeg",
            ImageKind::Svg => "image/svg+xml",
        }
    }
}

/// Type d'image d'après les premiers octets.
pub fn sniff(bytes: &[u8]) -> Option<ImageKind> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some(ImageKind::Png)
    } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some(ImageKind::Webp)
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some(ImageKind::Jpeg)
    } else if is_svg(bytes) {
        Some(ImageKind::Svg)
    } else {
        None
    }
}

/// SVG : texte dont la première balise (après un éventuel prologue XML) est `<svg`.
fn is_svg(bytes: &[u8]) -> bool {
    // Début du fichier seulement : un caractère accentué coupé à la limite n'est pas une erreur.
    let head = &bytes[..bytes.len().min(1024)];
    let text = match std::str::from_utf8(head) {
        Ok(t) => t,
        Err(e) if e.error_len().is_none() => std::str::from_utf8(&head[..e.valid_up_to()]).unwrap_or(""),
        Err(_) => return false,
    };
    let mut rest = text.trim_start_matches('\u{feff}').trim_start();
    loop {
        if rest.starts_with("<?") || rest.starts_with("<!--") || rest.starts_with("<!DOCTYPE") || rest.starts_with("<!doctype") {
            let end = if rest.starts_with("<!--") { rest.find("-->").map(|i| i + 3) } else { rest.find('>').map(|i| i + 1) };
            match end {
                Some(i) => rest = rest[i..].trim_start(),
                None => return false,
            }
        } else {
            return rest.starts_with("<svg");
        }
    }
}

pub fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

#[derive(Debug, Clone)]
pub struct IconCache {
    dir: PathBuf,
}

impl IconCache {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// Un fichier par adresse : changer l'adresse dans le catalogue change l'icône.
    fn path_for(&self, url: &str) -> PathBuf {
        let digest = hash::sha256_hex(url.as_bytes());
        self.dir.join(format!("{}.img", &digest[..32]))
    }

    /// Icône déjà en cache, en adresse `data:`.
    pub fn data_url(&self, url: &str) -> Option<String> {
        let bytes = fs::read(self.path_for(url)).ok()?;
        let kind = sniff(&bytes)?;
        Some(format!("data:{};base64,{}", kind.mime(), base64(&bytes)))
    }

    /// Télécharge l'icône si elle n'est pas en cache. Renvoie vrai si elle vient d'arriver.
    pub fn fetch(&self, http: &dyn HttpGet, url: &str) -> Result<bool, String> {
        let path = self.path_for(url);
        if path.exists() {
            return Ok(false);
        }
        let body = match http.get(url, None)? {
            HttpResponse::Ok { body, .. } => body,
            HttpResponse::NotModified => return Ok(false),
        };
        if body.len() > MAX_ICON_BYTES {
            return Err(format!("icône trop lourde ({} Ko)", body.len() / 1024));
        }
        if sniff(&body).is_none() {
            return Err("icône refusée : ni PNG, ni WebP, ni JPEG, ni SVG".into());
        }
        fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
        storage::write_bytes_atomic(&path, &body).map_err(|e| e.to_string())?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Fake(Vec<u8>, AtomicUsize);
    impl HttpGet for Fake {
        fn get(&self, _url: &str, _etag: Option<&str>) -> Result<HttpResponse, String> {
            self.1.fetch_add(1, Ordering::SeqCst);
            Ok(HttpResponse::Ok { body: self.0.clone(), etag: None })
        }
    }

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\nreste";

    #[test]
    fn svg_avec_accent_coupe_a_la_limite() {
        // « é » (2 octets) à cheval sur l'octet 1024 : reste reconnu comme SVG.
        let mut svg = b"<svg xmlns=\"http://www.w3.org/2000/svg\"><!--".to_vec();
        svg.resize(1023, b'a');
        svg.extend_from_slice("é--></svg>".as_bytes());
        assert!(is_svg(&svg));
        assert!(!is_svg(&[0xff, 0xfe, b'<', b's']));
    }

    #[test]
    fn signatures() {
        assert_eq!(sniff(PNG), Some(ImageKind::Png));
        assert_eq!(sniff(b"RIFF\0\0\0\0WEBPVP8 "), Some(ImageKind::Webp));
        assert_eq!(sniff(&[0xFF, 0xD8, 0xFF, 0xE0]), Some(ImageKind::Jpeg));
        assert_eq!(sniff(b"<svg xmlns='http://www.w3.org/2000/svg'/>"), Some(ImageKind::Svg));
        assert_eq!(sniff(b"<?xml version='1.0'?>\n<!-- x -->\n<svg/>"), Some(ImageKind::Svg));
        assert_eq!(sniff(b"<html><svg/></html>"), None);
        assert_eq!(sniff(b""), None);
    }

    #[test]
    fn encodage_base64() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn cache_et_refus() {
        let dir = tempfile::tempdir().unwrap();
        let cache = IconCache::new(dir.path().join("icons"));
        let url = "https://bworkshop.app/assets/forge.png";
        assert_eq!(cache.data_url(url), None);
        let http = Fake(PNG.to_vec(), AtomicUsize::new(0));
        assert_eq!(cache.fetch(&http, url), Ok(true));
        assert_eq!(cache.fetch(&http, url), Ok(false), "pas de second téléchargement");
        assert_eq!(http.1.load(Ordering::SeqCst), 1);
        assert!(cache.data_url(url).unwrap().starts_with("data:image/png;base64,"));

        let html = Fake(b"<html>".to_vec(), AtomicUsize::new(0));
        assert!(cache.fetch(&html, "https://bworkshop.app/a.svg").is_err());
        let lourd = Fake([PNG, &vec![0u8; MAX_ICON_BYTES]].concat(), AtomicUsize::new(0));
        assert!(cache.fetch(&lourd, "https://bworkshop.app/b.png").is_err());
        assert_eq!(cache.data_url("https://bworkshop.app/a.svg"), None);
    }
}
