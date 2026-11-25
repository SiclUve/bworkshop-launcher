//! Lecture et écriture de fichiers JSON.
//!
//! L'écriture est atomique : on écrit dans un fichier temporaire du même
//! dossier, on force l'écriture sur disque, puis on le renomme par-dessus la
//! cible (`MoveFileExW` avec remplacement sous Windows). Une coupure de courant
//! laisse donc soit l'ancien fichier, soit le nouveau, jamais un mélange.

use serde::Serialize;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("erreur d'entrée/sortie : {0}")]
    Io(#[from] io::Error),
    #[error("JSON invalide : {0}")]
    Json(#[from] serde_json::Error),
}

/// Écrit `value` en JSON dans `path`, de façon atomique.
pub fn write_json_atomic<T: Serialize + ?Sized>(path: &Path, value: &T) -> Result<(), StorageError> {
    let dir = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "chemin sans dossier parent"))?;
    fs::create_dir_all(dir)?;

    let mut tmp = tempfile::Builder::new()
        .prefix(".tmp-")
        .suffix(".json")
        .tempfile_in(dir)?;
    serde_json::to_writer_pretty(tmp.as_file_mut(), value)?;
    tmp.as_file_mut().write_all(b"\n")?;
    tmp.as_file().sync_all()?;
    tmp.persist(path).map_err(|e| e.error)?;
    Ok(())
}

/// Écrit des octets dans `path`, de façon atomique.
pub fn write_bytes_atomic(path: &Path, bytes: &[u8]) -> Result<(), StorageError> {
    let dir = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "chemin sans dossier parent"))?;
    fs::create_dir_all(dir)?;
    let mut tmp = tempfile::Builder::new().prefix(".tmp-").tempfile_in(dir)?;
    tmp.as_file_mut().write_all(bytes)?;
    tmp.as_file().sync_all()?;
    tmp.persist(path).map_err(|e| e.error)?;
    Ok(())
}

/// Renomme un fichier illisible en `<nom>.corrupt-<horodatage>` pour le garder
/// de côté (diagnostic) sans qu'il bloque le démarrage.
pub fn backup_corrupt(path: &Path) -> io::Result<PathBuf> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mut name = path
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_else(|| "fichier".into());
    name.push(format!(".corrupt-{stamp}"));
    let backup = path.with_file_name(name);
    fs::rename(path, &backup)?;
    Ok(backup)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fichiers(dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    }

    #[test]
    fn ecriture_puis_relecture() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sous").join("a.json");
        write_json_atomic(&path, &json!({"x": 1})).unwrap();
        let v: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(v["x"], 1);
    }

    #[test]
    fn remplace_le_fichier_existant_sans_laisser_de_temporaire() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.json");
        write_json_atomic(&path, &json!({"x": 1})).unwrap();
        write_json_atomic(&path, &json!({"x": 2})).unwrap();
        let v: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(v["x"], 2);
        assert_eq!(fichiers(dir.path()), vec!["a.json".to_string()]);
    }

    #[test]
    fn sauvegarde_d_un_fichier_corrompu() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.json");
        fs::write(&path, b"{pas du json").unwrap();
        let backup = backup_corrupt(&path).unwrap();
        assert!(!path.exists());
        assert!(backup.exists());
        assert!(backup.file_name().unwrap().to_string_lossy().starts_with("a.json.corrupt-"));
    }
}
