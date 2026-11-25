//! Type d'installateur, reconnu par sa signature (jamais par son nom ou son extension).

use super::{inno, nsis};
use serde::Serialize;
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

/// Zone lue pour reconnaître un installateur (les signatures sont au début du fichier).
const SCAN_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum InstallerKind {
    Inno,
    Nsis,
}

impl InstallerKind {
    pub fn label(self) -> &'static str {
        match self {
            InstallerKind::Inno => "Inno Setup",
            InstallerKind::Nsis => "NSIS",
        }
    }
}

pub(crate) fn read_head(path: &Path) -> io::Result<Vec<u8>> {
    let mut buf = Vec::new();
    File::open(path)?.take(SCAN_BYTES).read_to_end(&mut buf)?;
    Ok(buf)
}

pub(crate) fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// Inno, NSIS, ou `None` (fichier seulement téléchargé, dossier ouvert).
pub fn detect(path: &Path) -> io::Result<Option<InstallerKind>> {
    let head = read_head(path)?;
    Ok(if inno::matches(&head) {
        Some(InstallerKind::Inno)
    } else if nsis::matches(&head) {
        Some(InstallerKind::Nsis)
    } else {
        None
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn asset(rel: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-assets").join(rel)
    }

    #[test]
    fn types_reconnus() {
        // Installateurs de test : construits par test-assets/*/build.ps1, non versionnés.
        let (inno, nsis) = (asset("inno/TestTool_Setup_1.0.0.exe"), asset("nsis/NsisTool_Setup_1.0.0.exe"));
        if inno.exists() && nsis.exists() {
            assert_eq!(detect(&inno).unwrap(), Some(InstallerKind::Inno));
            assert_eq!(detect(&nsis).unwrap(), Some(InstallerKind::Nsis));
        }
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.zip");
        std::fs::write(&p, b"PK\x03\x04").unwrap();
        assert_eq!(detect(&p).unwrap(), None);
    }
}
