//! Journal local.
//!
//! - Fichier : `%LOCALAPPDATA%\BworkshopLauncher\logs\launcher.log`.
//! - Rotation à 5 Mo : l'ancien fichier devient `launcher.1.log` (un seul gardé).
//! - Rien n'est envoyé sur internet.
//! - Chaque message est nettoyé : le dossier du profil Windows est remplacé
//!   par `%USERPROFILE%` et les adresses e-mail par `<e-mail>`.

use log::{Level, LevelFilter, Log, Metadata, Record};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// Taille maximale d'un fichier de journal.
pub const MAX_LOG_BYTES: u64 = 5 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum LogInitError {
    #[error("journal : {0}")]
    Io(#[from] io::Error),
    #[error("journal déjà initialisé")]
    AlreadySet,
}

/// Retire les données personnelles d'un message de journal.
#[derive(Debug, Clone, Default)]
pub struct Sanitizer {
    /// Dossier du profil, en minuscules ASCII, sous ses deux écritures (\ et /).
    profile_variants: Vec<String>,
}

impl Sanitizer {
    pub fn from_env() -> Self {
        Self::new(std::env::var("USERPROFILE").ok().as_deref())
    }

    pub fn new(profile: Option<&str>) -> Self {
        let mut profile_variants = Vec::new();
        if let Some(p) = profile {
            let p = p.trim_end_matches(['\\', '/']).to_ascii_lowercase();
            // Garde-fou : on ne remplace pas un « profil » trop court comme « C: ».
            if p.len() > 3 {
                profile_variants.push(p.replace('/', "\\"));
                profile_variants.push(p.replace('\\', "/"));
            }
        }
        Self { profile_variants }
    }

    pub fn clean(&self, message: &str) -> String {
        let mut s = message.to_string();
        for variant in &self.profile_variants {
            s = replace_ascii_case_insensitive(&s, variant, "%USERPROFILE%");
        }
        mask_emails(&s)
    }
}

/// Remplace `needle` (déjà en minuscules ASCII) sans tenir compte de la casse ASCII.
/// `to_ascii_lowercase` ne change pas la longueur en octets : les indices restent valides.
fn replace_ascii_case_insensitive(haystack: &str, needle: &str, replacement: &str) -> String {
    if needle.is_empty() {
        return haystack.to_string();
    }
    let lower = haystack.to_ascii_lowercase();
    let mut out = String::with_capacity(haystack.len());
    let mut last = 0;
    let mut from = 0;
    while let Some(pos) = lower[from..].find(needle) {
        let start = from + pos;
        out.push_str(&haystack[last..start]);
        out.push_str(replacement);
        last = start + needle.len();
        from = last;
    }
    out.push_str(&haystack[last..]);
    out
}

fn is_local_part(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'%' | b'+' | b'-')
}

fn is_domain_part(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-')
}

/// Masque tout ce qui ressemble à une adresse e-mail.
pub fn mask_emails(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut last = 0;
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'@' {
            let mut l = i;
            while l > last && is_local_part(b[l - 1]) {
                l -= 1;
            }
            let mut r = i + 1;
            while r < b.len() && is_domain_part(b[r]) {
                r += 1;
            }
            while r > i + 1 && b[r - 1] == b'.' {
                r -= 1;
            }
            let domain = &s[i + 1..r];
            if l < i && domain.contains('.') && !domain.starts_with('.') {
                out.push_str(&s[last..l]);
                out.push_str("<e-mail>");
                last = r;
                i = r;
                continue;
            }
        }
        i += 1;
    }
    out.push_str(&s[last..]);
    out
}

/// Fichier avec rotation par taille.
pub struct RotatingFile {
    path: PathBuf,
    file: Option<File>,
    size: u64,
    max: u64,
}

impl RotatingFile {
    pub fn open(path: PathBuf, max: u64) -> io::Result<Self> {
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        let size = file.metadata()?.len();
        Ok(Self { path, file: Some(file), size, max })
    }

    fn rotated_path(&self) -> PathBuf {
        let stem = self.path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        self.path.with_file_name(format!("{stem}.1.log"))
    }

    pub fn write_line(&mut self, line: &str) -> io::Result<()> {
        let len = line.len() as u64;
        if self.size > 0 && self.size + len > self.max {
            // Rotation ratée mais journal rouvert : on continue d'écrire dedans.
            if let Err(e) = self.rotate()
                && self.file.is_none() {
                    return Err(e);
                }
        }
        let file = match self.file.as_mut() {
            Some(f) => f,
            None => return Err(io::Error::other("fichier de journal fermé")),
        };
        file.write_all(line.as_bytes())?;
        self.size += len;
        Ok(())
    }

    fn rotate(&mut self) -> io::Result<()> {
        self.file = None; // fermer avant de renommer (obligatoire sous Windows)
        let old = self.rotated_path();
        let _ = fs::remove_file(&old);
        let renamed = fs::rename(&self.path, &old);
        // Renommage raté (fichier ouvert ailleurs…) : on rouvre quand même le journal pour ne
        // pas devenir muet ; le prochain essai aura lieu après `max` octets de plus.
        self.file = Some(OpenOptions::new().create(true).append(true).open(&self.path)?);
        self.size = 0;
        renamed
    }
}

struct FileLogger {
    file: Mutex<RotatingFile>,
    level: LevelFilter,
    sanitizer: Sanitizer,
}

impl Log for FileLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        if metadata.level() > self.level {
            return false;
        }
        // Les librairies tierces ne remontent que les avertissements et erreurs.
        metadata.target().starts_with("bworkshop") || metadata.level() <= Level::Warn
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let line = format!(
            "{} {:<5} [{}] {}\n",
            timestamp_utc(SystemTime::now()),
            record.level(),
            record.target(),
            self.sanitizer.clean(&record.args().to_string())
        );
        if cfg!(debug_assertions) {
            eprint!("{line}");
        }
        if let Ok(mut f) = self.file.lock() {
            let _ = f.write_line(&line);
        }
    }

    fn flush(&self) {
        if let Ok(mut f) = self.file.lock()
            && let Some(file) = f.file.as_mut() {
                let _ = file.flush();
            }
    }
}

/// Installe le journal global. À appeler une seule fois, au démarrage.
pub fn init(dir: &Path, level: LevelFilter) -> Result<(), LogInitError> {
    fs::create_dir_all(dir)?;
    let file = RotatingFile::open(dir.join("launcher.log"), MAX_LOG_BYTES)?;
    let logger = FileLogger {
        file: Mutex::new(file),
        level,
        sanitizer: Sanitizer::from_env(),
    };
    log::set_boxed_logger(Box::new(logger)).map_err(|_| LogInitError::AlreadySet)?;
    log::set_max_level(level);

    // Une panique est écrite dans le journal avant le comportement par défaut.
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log::error!("Panique : {info}");
        log::logger().flush();
        previous(info);
    }));
    Ok(())
}

/// Horodatage UTC au format ISO 8601, sans dépendance externe.
pub fn timestamp_utc(now: SystemTime) -> String {
    let secs = now.duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let (y, m, d) = civil_from_days((secs / 86_400) as i64);
    let rem = secs % 86_400;
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// Jours depuis 1970-01-01 → (année, mois, jour). Algorithme de H. Hinnant.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe as i64 + era * 400 + if m <= 2 { 1 } else { 0 };
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn masque_le_dossier_du_profil_sans_tenir_compte_de_la_casse() {
        let s = Sanitizer::new(Some(r"C:\Users\Jean"));
        assert_eq!(
            s.clean(r"Ouverture de c:\users\JEAN\AppData\x.json"),
            r"Ouverture de %USERPROFILE%\AppData\x.json"
        );
        assert_eq!(s.clean("C:/Users/Jean/a"), "%USERPROFILE%/a");
    }

    #[test]
    fn profil_trop_court_ignore() {
        let s = Sanitizer::new(Some("C:"));
        assert_eq!(s.clean(r"C:\a"), r"C:\a");
    }

    #[test]
    fn masque_les_e_mails() {
        assert_eq!(mask_emails("contact : jean.dupont+x@exemple.fr."), "contact : <e-mail>.");
        assert_eq!(mask_emails("a@b.c et d@e.f"), "<e-mail> et <e-mail>");
        assert_eq!(mask_emails("pas@domaine et @rien"), "pas@domaine et @rien");
        // Partie locale non ASCII : pas reconnue, laissée telle quelle.
        assert_eq!(mask_emails("é@x.fr"), "é@x.fr");
    }

    #[test]
    fn rotation_a_la_taille_maximale() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("launcher.log");
        let mut f = RotatingFile::open(path.clone(), 20).unwrap();
        f.write_line("0123456789\n").unwrap(); // 11 octets
        f.write_line("abcdefghij\n").unwrap(); // 22 > 20 → rotation avant
        assert_eq!(fs::read_to_string(&path).unwrap(), "abcdefghij\n");
        assert_eq!(
            fs::read_to_string(dir.path().join("launcher.1.log")).unwrap(),
            "0123456789\n"
        );
        f.write_line("ZZZZZZZZZZ\n").unwrap(); // nouvelle rotation, l'ancien .1 est remplacé
        assert_eq!(
            fs::read_to_string(dir.path().join("launcher.1.log")).unwrap(),
            "abcdefghij\n"
        );
    }

    #[test]
    fn horodatage() {
        assert_eq!(timestamp_utc(UNIX_EPOCH), "1970-01-01T00:00:00Z");
        let t = UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        assert_eq!(timestamp_utc(t), "2023-11-14T22:13:20Z");
        let t = UNIX_EPOCH + Duration::from_secs(951_782_400); // 29 février 2000
        assert_eq!(timestamp_utc(t), "2000-02-29T00:00:00Z");
    }
}
