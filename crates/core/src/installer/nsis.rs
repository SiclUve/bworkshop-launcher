//! Installateurs NSIS (Nullsoft) : reconnaissance, arguments silencieux, codes de sortie.
//!
//! Utilisés par exemple par les applications Tauri (installation « pour l'utilisateur »).
//! Un installateur NSIS ne crée pas de clé `_is1` : il est retrouvé dans le registre
//! par son nom affiché.

use super::kind::{contains, read_head};
use std::io;
use std::path::Path;

pub const SILENT_INSTALL_ARGS: &[&str] = &["/S"];
pub const SILENT_UNINSTALL_ARGS: &[&str] = &["/S"];

/// Signature de l'en-tête de données NSIS (installateurs et désinstalleurs).
const MARKER: &[u8] = b"NullsoftInst";

pub(crate) fn matches(head: &[u8]) -> bool {
    head.starts_with(b"MZ") && contains(head, MARKER)
}

/// Le fichier est-il un exécutable NSIS (installateur ou désinstalleur) ?
pub fn is_nsis(path: &Path) -> io::Result<bool> {
    Ok(matches(&read_head(path)?))
}

/// Codes de sortie documentés de NSIS.
pub fn exit_code_meaning(code: i32) -> &'static str {
    match code {
        0 => "terminé",
        1 => "annulé",
        2 => "interrompu par l'installateur",
        _ => "code inconnu",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signature() {
        assert!(matches(b"MZ....\xEF\xBE\xAD\xDENullsoftInst...."));
        assert!(!matches(b"NullsoftInst sans en-tete MZ"));
        assert!(!matches(b"MZ rien de plus"));
    }

    #[test]
    fn installateur_de_test_reconnu() {
        let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-assets/nsis/NsisTool_Setup_1.0.0.exe");
        if !p.exists() {
            eprintln!("ignoré : {} absent (lancer le build.ps1 de test-assets)", p.display());
            return;
        }
        assert!(is_nsis(&p).unwrap());
    }
}
