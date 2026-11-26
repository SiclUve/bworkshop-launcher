//! Téléchargement en flux, avec progression, annulation et reprise.
//!
//! - Le SHA-256 est calculé pendant le téléchargement (et sur la partie déjà
//!   téléchargée en cas de reprise).
//! - Le fichier est écrit en `<nom>.part`, renommé seulement s'il est valide.
//! - Empreinte différente : le fichier est supprimé, jamais lancé.
//! - Plus de 5 % au-dessus de la taille annoncée : arrêt et suppression.
//! - Seules les adresses de la liste autorisée sont contactées (redirections comprises).

use crate::net;
use crate::security::domains::{self, DomainError};
use crate::security::hash::{self, to_hex};
use reqwest::blocking::Client;
use reqwest::header::{CONTENT_LENGTH, RANGE};
use reqwest::StatusCode;
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum DownloadError {
    #[error("{0}")]
    Domain(#[from] DomainError),
    #[error("erreur réseau : {0}")]
    Network(String),
    #[error("le serveur a répondu {0}")]
    Http(u16),
    #[error("erreur d'écriture : {0}")]
    Io(#[from] io::Error),
    #[error("fichier plus gros que prévu (plus de {max} octets) : téléchargement arrêté et supprimé")]
    TooLarge { max: u64 },
    #[error("l'empreinte SHA-256 du fichier ne correspond pas à celle publiée : fichier supprimé, il ne sera pas lancé")]
    HashMismatch { expected: String, actual: String },
    #[error("téléchargement annulé")]
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct DownloadRequest {
    pub url: String,
    /// Fichier final (le temporaire est `<dest>.part`).
    pub dest: PathBuf,
    /// Empreinte publiée, obligatoire : aucun fichier n'est gardé sans vérification.
    pub expected_sha256: String,
    pub max_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    pub received: u64,
    pub total: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Downloaded {
    pub path: PathBuf,
    pub sha256: String,
}

fn sidecar(dest: &Path, ext: &str) -> PathBuf {
    let mut s = dest.as_os_str().to_os_string();
    s.push(ext);
    PathBuf::from(s)
}

/// Délai d'attente de chaque opération (réponse du serveur, puis **chaque** lecture du
/// corps) : avec le client bloquant de reqwest, il ne limite pas la durée totale d'un gros
/// téléchargement, mais une connexion muette s'arrête (et « Annuler » répond) en 30 s au plus.
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(30);

/// Client pour les gros fichiers : délai de connexion, et délai par lecture (pas global).
pub fn download_client() -> Result<Client, DownloadError> {
    net::install_tls_provider();
    Client::builder()
        .https_only(true)
        .redirect(net::redirect_policy())
        .user_agent(net::user_agent())
        .connect_timeout(Duration::from_secs(15))
        .timeout(IDLE_TIMEOUT)
        .build()
        .map_err(|e| DownloadError::Network(e.to_string()))
}

/// Copie un flux dans un fichier en calculant l'empreinte. Testable sans réseau.
pub fn copy_stream(
    reader: &mut dyn Read,
    file: &mut File,
    hasher: &mut Sha256,
    received: &mut u64,
    max_bytes: u64,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(u64),
) -> Result<(), DownloadError> {
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(DownloadError::Cancelled);
        }
        let n = reader.read(&mut buf).map_err(|e| DownloadError::Network(e.to_string()))?;
        if n == 0 {
            return Ok(());
        }
        *received += n as u64;
        if *received > max_bytes {
            return Err(DownloadError::TooLarge { max: max_bytes });
        }
        file.write_all(&buf[..n])?;
        hasher.update(&buf[..n]);
        progress(*received);
    }
}

/// Termine : compare l'empreinte, puis renomme `.part` en fichier final.
pub fn finish(req: &DownloadRequest, part: &Path, digest: String) -> Result<Downloaded, DownloadError> {
    let meta = sidecar(&req.dest, ".part.meta");
    if !hash::same_digest(&req.expected_sha256, &digest) {
        let _ = fs::remove_file(part);
        let _ = fs::remove_file(&meta);
        log::warn!("Empreinte SHA-256 différente pour {} : fichier supprimé", req.dest.display());
        return Err(DownloadError::HashMismatch { expected: req.expected_sha256.clone(), actual: digest });
    }
    if req.dest.exists() {
        fs::remove_file(&req.dest)?;
    }
    fs::rename(part, &req.dest)?;
    let _ = fs::remove_file(&meta);
    Ok(Downloaded { path: req.dest.clone(), sha256: digest })
}

/// Télécharge `req.url` vers `req.dest`.
pub fn download(
    client: &Client,
    req: &DownloadRequest,
    cancel: &AtomicBool,
    mut progress: impl FnMut(Progress),
) -> Result<Downloaded, DownloadError> {
    let url = domains::check_initial_str(&req.url).inspect_err(|e| log::warn!("Téléchargement refusé : {e}"))?;
    if let Some(dir) = req.dest.parent() {
        fs::create_dir_all(dir)?;
    }
    let part = sidecar(&req.dest, ".part");
    let meta = sidecar(&req.dest, ".part.meta");
    // La reprise n'est possible que pour le même fichier (même adresse, même empreinte).
    let identity = format!("{}\n{}", req.url, req.expected_sha256);

    let mut hasher = Sha256::new();
    let mut existing = 0u64;
    if part.exists() && fs::read_to_string(&meta).map(|m| m == identity).unwrap_or(false) {
        existing = hash::update_from_file(&mut hasher, &part)?;
        log::info!("Reprise du téléchargement à {existing} octets");
    } else {
        let _ = fs::remove_file(&part);
    }
    fs::write(&meta, &identity)?;

    let mut request = client.get(url);
    if existing > 0 {
        request = request.header(RANGE, format!("bytes={existing}-"));
    }
    let mut response = request.send().map_err(|e| DownloadError::Network(e.to_string()))?;

    // Partie déjà complète (annulation pendant le dernier bloc) : le serveur répond
    // « plage impossible ». On vérifie ce qu'on a : bonne empreinte, c'est fini ; sinon la
    // partie est supprimée par `finish` et le prochain essai repart de zéro.
    if response.status() == StatusCode::RANGE_NOT_SATISFIABLE && existing > 0 {
        log::info!("Partie déjà complète ({existing} octets) : vérification de l'empreinte");
        return finish(req, &part, to_hex(&hasher.finalize()));
    }

    let append = match response.status() {
        StatusCode::PARTIAL_CONTENT if existing > 0 => true,
        s if s.is_success() => false,
        s => return Err(DownloadError::Http(s.as_u16())),
    };
    if !append && existing > 0 {
        log::info!("Le serveur ne reprend pas : téléchargement depuis le début");
        hasher = Sha256::new();
        existing = 0;
    }

    let remaining = response
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok());
    let total = remaining.map(|r| r + existing);
    if let Some(t) = total
        && t > req.max_bytes {
            let _ = fs::remove_file(&part);
            let _ = fs::remove_file(&meta);
            return Err(DownloadError::TooLarge { max: req.max_bytes });
        }

    let mut file = if append {
        OpenOptions::new().append(true).open(&part)?
    } else {
        File::create(&part)?
    };
    let mut received = existing;
    progress(Progress { received, total });
    let result = copy_stream(&mut response, &mut file, &mut hasher, &mut received, req.max_bytes, cancel, &mut |r| {
        progress(Progress { received: r, total })
    });
    match result {
        Ok(()) => {}
        Err(DownloadError::TooLarge { max }) => {
            drop(file);
            let _ = fs::remove_file(&part);
            let _ = fs::remove_file(&meta);
            return Err(DownloadError::TooLarge { max });
        }
        // Annulation ou coupure : la partie téléchargée est gardée pour la reprise.
        Err(e) => return Err(e),
    }
    file.sync_all()?;
    drop(file);
    finish(req, &part, to_hex(&hasher.finalize()))
}

/// Vide le dossier propre à un outil avant un téléchargement, sauf la partie déjà
/// téléchargée de `keep` (reprise). Aucun fichier déposé à côté de l'installateur
/// (une DLL par exemple) ne peut ainsi être chargé par lui.
pub fn clean_tool_dir(dir: &Path, keep: &Path) -> io::Result<()> {
    let keep_part = sidecar(keep, ".part");
    let keep_meta = sidecar(keep, ".part.meta");
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    for entry in entries {
        let path = entry?.path();
        if path == keep_part || path == keep_meta {
            continue;
        }
        if path.is_dir() {
            fs::remove_dir_all(&path)?;
        } else {
            fs::remove_file(&path)?;
        }
    }
    Ok(())
}

/// Ouvre le fichier en interdisant toute écriture tant que le résultat est gardé,
/// puis recalcule son empreinte. À garder ouvert jusqu'à la fin de l'installateur :
/// le fichier lancé est forcément celui qui a été vérifié.
pub fn open_verified(path: &Path, expected: &str) -> Result<File, DownloadError> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(0x1); // FILE_SHARE_READ : lecture seule pour les autres
    }
    let mut file = options.open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let actual = to_hex(&hasher.finalize());
    if !hash::same_digest(expected, &actual) {
        drop(file);
        let _ = fs::remove_file(path);
        log::warn!("Empreinte changée avant le lancement de {} : fichier supprimé", path.display());
        return Err(DownloadError::HashMismatch { expected: expected.to_string(), actual });
    }
    Ok(file)
}

/// Nom de fichier sûr tiré de l'adresse (dernier segment), sinon `<id>.exe`.
pub fn file_name_for(url: &str, id: &str) -> String {
    let last = url
        .split(['?', '#'])
        .next()
        .and_then(|u| u.rsplit('/').next())
        .unwrap_or("");
    let clean: String = last
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        .collect();
    let clean = clean.trim_start_matches('.').to_string();
    if clean.is_empty() || clean.len() > 120 {
        format!("{id}.exe")
    } else {
        clean
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_tool_dir_keeps_only_resume_part() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("a.exe");
        fs::write(dir.path().join("a.exe.part"), b"x").unwrap();
        fs::write(dir.path().join("a.exe.part.meta"), b"x").unwrap();
        fs::write(dir.path().join("version.dll"), b"x").unwrap();
        fs::create_dir(dir.path().join("sub")).unwrap();
        clean_tool_dir(dir.path(), &dest).unwrap();
        let mut left: Vec<_> = fs::read_dir(dir.path()).unwrap().map(|e| e.unwrap().file_name()).collect();
        left.sort();
        assert_eq!(left, ["a.exe.part", "a.exe.part.meta"]);
        clean_tool_dir(&dir.path().join("absent"), &dest).unwrap();
    }

    #[test]
    fn open_verified_checks_and_locks() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("a.exe");
        fs::write(&f, b"abc").unwrap();
        let good = hash::sha256_hex(b"abc");
        let held = open_verified(&f, &good).unwrap();
        #[cfg(windows)]
        assert!(OpenOptions::new().write(true).open(&f).is_err(), "écriture possible pendant la vérification");
        drop(held);
        fs::write(&f, b"abd").unwrap();
        assert!(matches!(open_verified(&f, &good), Err(DownloadError::HashMismatch { .. })));
        assert!(!f.exists());
    }
    use std::io::Cursor;

    fn req(dir: &Path, sha: String, max: u64) -> DownloadRequest {
        DownloadRequest {
            url: "https://bworkshop.app/a.exe".into(),
            dest: dir.join("a.exe"),
            expected_sha256: sha,
            max_bytes: max,
        }
    }

    /// Simule un téléchargement complet à partir d'un contenu en mémoire.
    fn simulate(r: &DownloadRequest, content: &[u8], cancel: bool) -> Result<Downloaded, DownloadError> {
        let part = sidecar(&r.dest, ".part");
        let mut file = File::create(&part)?;
        let mut hasher = Sha256::new();
        let mut received = 0;
        let flag = AtomicBool::new(cancel);
        copy_stream(&mut Cursor::new(content), &mut file, &mut hasher, &mut received, r.max_bytes, &flag, &mut |_| {})?;
        drop(file);
        finish(r, &part, to_hex(&hasher.finalize()))
    }

    #[test]
    fn empreinte_correcte_acceptee() {
        let dir = tempfile::tempdir().unwrap();
        let content = b"installateur".repeat(10_000);
        let r = req(dir.path(), hash::sha256_hex(&content).to_uppercase(), 1 << 30);
        let d = simulate(&r, &content, false).unwrap();
        assert_eq!(fs::read(&d.path).unwrap(), content);
        assert!(!sidecar(&r.dest, ".part").exists());
    }

    #[test]
    fn fichier_modifie_refuse_et_supprime() {
        let dir = tempfile::tempdir().unwrap();
        let r = req(dir.path(), hash::sha256_hex(b"original"), 1 << 30);
        let err = simulate(&r, b"original modifie", false).unwrap_err();
        assert!(matches!(err, DownloadError::HashMismatch { .. }));
        assert!(!r.dest.exists());
        assert!(!sidecar(&r.dest, ".part").exists());
    }

    #[test]
    fn trop_gros_refuse() {
        let dir = tempfile::tempdir().unwrap();
        let r = req(dir.path(), hash::sha256_hex(&[0u8; 100]), 100);
        assert!(matches!(simulate(&r, &[0u8; 101], false), Err(DownloadError::TooLarge { max: 100 })));
        assert!(simulate(&r, &[0u8; 100], false).is_ok());
    }

    #[test]
    fn annulation() {
        let dir = tempfile::tempdir().unwrap();
        let r = req(dir.path(), hash::sha256_hex(b"x"), 1 << 30);
        assert!(matches!(simulate(&r, b"x", true), Err(DownloadError::Cancelled)));
        assert!(!r.dest.exists());
    }

    #[test]
    fn reprise_empreinte_sur_les_deux_parties() {
        let dir = tempfile::tempdir().unwrap();
        let content = b"0123456789".repeat(1000);
        let r = req(dir.path(), hash::sha256_hex(&content), 1 << 30);
        let part = sidecar(&r.dest, ".part");
        fs::write(&part, &content[..4000]).unwrap();
        let mut hasher = Sha256::new();
        let mut received = hash::update_from_file(&mut hasher, &part).unwrap();
        let mut file = OpenOptions::new().append(true).open(&part).unwrap();
        let flag = AtomicBool::new(false);
        copy_stream(&mut Cursor::new(&content[4000..]), &mut file, &mut hasher, &mut received, r.max_bytes, &flag, &mut |_| {})
            .unwrap();
        drop(file);
        let d = finish(&r, &part, to_hex(&hasher.finalize())).unwrap();
        assert_eq!(fs::read(d.path).unwrap(), content);
    }

    #[test]
    fn adresse_hors_liste_refusee_avant_tout_acces() {
        let dir = tempfile::tempdir().unwrap();
        let mut r = req(dir.path(), "0".repeat(64), 10);
        r.url = "https://evil.example/a.exe".into();
        let client = download_client().unwrap();
        let err = download(&client, &r, &AtomicBool::new(false), |_| {}).unwrap_err();
        assert!(matches!(err, DownloadError::Domain(DomainError::NotAllowed(_))));
        assert!(!sidecar(&r.dest, ".part").exists());
    }

    #[test]
    fn noms_de_fichier() {
        assert_eq!(
            file_name_for("https://github.com/a/b/releases/download/v1/GrimoireBoss-Setup.exe", "x"),
            "GrimoireBoss-Setup.exe"
        );
        assert_eq!(file_name_for("https://bworkshop.app/api/downloads/", "outil"), "outil.exe");
        assert_eq!(file_name_for("https://x/..%2F..%2Fwin.exe?y=1", "o"), "2F..2Fwin.exe");
    }
}
