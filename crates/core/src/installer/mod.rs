//! Installation des outils : téléchargement, vérification, installation
//! silencieuse (Inno Setup ou NSIS), détection dans le registre, mise à jour,
//! désinstallation et lancement.

pub mod download;
pub mod inno;
pub mod kind;
pub mod known_apps;
pub mod launch;
pub mod nsis;
pub mod registry;
pub mod runner;
pub mod status;
pub mod version;

/// Plafond pour un fichier sans taille annoncée (plugins communautaires).
pub const MAX_UNSIZED_DOWNLOAD: u64 = 1024 * 1024 * 1024;

/// Taille maximale acceptée : 5 % au-dessus de la taille annoncée.
pub fn max_download_size(announced: Option<u64>) -> u64 {
    match announced {
        Some(n) => n.saturating_add(n.div_ceil(20)),
        None => MAX_UNSIZED_DOWNLOAD,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marge_de_cinq_pour_cent() {
        assert_eq!(max_download_size(Some(100)), 105);
        assert_eq!(max_download_size(Some(141_743_088)), 141_743_088 + 7_087_155);
        assert_eq!(max_download_size(None), MAX_UNSIZED_DOWNLOAD);
    }
}
