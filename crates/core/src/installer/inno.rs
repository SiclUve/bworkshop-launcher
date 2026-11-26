//! Installateurs Inno Setup : reconnaissance, arguments silencieux, codes de sortie.

use super::kind::contains;

/// `/CURRENTUSER` : installation pour l'utilisateur seul, sans droits administrateur, quand
/// l'installateur laisse le choix (`PrivilegesRequiredOverridesAllowed`). Un installateur Inno
/// qui exige les droits administrateur se relance lui-même avec élévation : Windows affiche
/// alors sa fenêtre UAC malgré `/VERYSILENT`. Le launcher ne la contourne pas : refusée, elle
/// arrête l'installation et l'erreur est affichée.
pub const SILENT_INSTALL_ARGS: &[&str] =
    &["/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/SP-", "/CLOSEAPPLICATIONS", "/CURRENTUSER"];
pub const SILENT_UNINSTALL_ARGS: &[&str] = &["/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART"];

/// Signature de la table du chargeur Inno (absente des désinstalleurs et des exe ordinaires).
const LOADER_MARKER: &[u8] = b"rDlPtS";
const DATA_MARKER: &[u8] = b"Inno Setup Setup Data";

pub(crate) fn matches(head: &[u8]) -> bool {
    head.starts_with(b"MZ") && contains(head, LOADER_MARKER) && contains(head, DATA_MARKER)
}

/// Codes de sortie documentés d'Inno Setup.
pub fn exit_code_meaning(code: i32) -> &'static str {
    match code {
        0 => "terminé",
        1 => "échec au démarrage",
        2 => "annulé avant le début",
        3 => "erreur fatale pendant la préparation",
        4 => "erreur fatale pendant l'installation",
        5 => "annulé pendant l'installation",
        6 => "arrêté de force",
        7 => "la préparation a échoué (un programme bloque peut-être les fichiers)",
        8 => "la préparation demande un redémarrage",
        _ => "code inconnu",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::installer::kind::read_head;
    use std::io;
    use std::path::Path;

    /// Le fichier est-il un installateur Inno Setup ? (L'application passe par `kind::detect`.)
    fn is_inno_installer(path: &Path) -> io::Result<bool> {
        Ok(matches(&read_head(path)?))
    }

    #[test]
    fn un_fichier_quelconque_n_est_pas_inno() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.zip");
        std::fs::write(&p, b"PK\x03\x04 pas un exe").unwrap();
        assert!(!is_inno_installer(&p).unwrap());
        let p = dir.path().join("b.exe");
        std::fs::write(&p, b"MZ Inno Setup Setup Data sans table du chargeur").unwrap();
        assert!(!is_inno_installer(&p).unwrap());
    }

    #[test]
    fn installateur_de_test_reconnu() {
        let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-assets/inno/TestTool_Setup_1.0.0.exe");
        if !p.exists() {
            eprintln!("ignoré : {} absent (lancer le build.ps1 de test-assets)", p.display());
            return;
        }
        assert!(is_inno_installer(&p).unwrap());
    }
}
