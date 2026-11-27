//! Outils sur Mac : des applications (`.app`) copiées depuis une image disque (`.dmg`)
//! dans `~/Applications` (dossier de l'utilisateur, sans mot de passe administrateur).
//!
//! - Installé = un `.app` de `~/Applications` dont l'identifiant (`CFBundleIdentifier`) est
//!   celui de l'outil (table [`bundle_id_for`]), sinon dont le nom affiché est celui du catalogue.
//!   Chaque application devient une [`UninstallEntry`] : le reste du launcher (états, mises à jour,
//!   lancement) fonctionne comme sous Windows.
//! - Installer = monter l'image en lecture seule (`hdiutil attach -nobrowse`), copier son unique
//!   `.app` (`ditto`, qui garde la signature), démonter. L'ancienne version est remplacée
//!   seulement une fois la copie réussie.
//! - Désinstaller = mettre l'application à la corbeille de l'utilisateur.

use super::registry::{RegistrySource, UninstallEntry};
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// Préfixe des clés des entrées Mac (`mac:app.bworkshop.grimoire`).
pub const KEY_PREFIX: &str = "mac:";

/// Identifiant d'application Mac de chaque outil du catalogue.
const BUNDLE_IDS: &[(&str, &str)] = &[
    ("simulateur-harebourg", "app.bworkshop.harebourg-overlay"),
    ("bworklingo", "app.bworkshop.bworklingo"),
    ("skin-helper", "app.bworkshop.dofus-color-picker"),
    ("grimoire", "app.bworkshop.grimoire"),
    ("grimoire-boss", "app.bworkshop.grimoire-boss"),
    ("grimoire-compagnon", "app.bworkshop.grimoire-compagnon"),
];

pub fn bundle_id_for(tool_id: &str) -> Option<&'static str> {
    BUNDLE_IDS.iter().find(|(id, _)| *id == tool_id).map(|(_, b)| *b)
}

/// Clé d'entrée correspondant à un identifiant d'application.
pub fn key_for(bundle_id: &str) -> String {
    format!("{KEY_PREFIX}{bundle_id}")
}

/// `~/Applications`.
pub fn user_applications_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| Path::new(&h).join("Applications"))
}

// ---------- Info.plist ----------

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}

/// Clés texte de premier niveau d'un `Info.plist` XML (`<key>…</key><string>…</string>`).
/// Les valeurs qui ne sont pas du texte (tableaux, booléens…) sont ignorées.
pub fn parse_plist_strings(xml: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let mut rest = xml;
    while let Some(start) = rest.find("<key>") {
        let after = &rest[start + 5..];
        let Some(end) = after.find("</key>") else { break };
        let key = unescape(after[..end].trim());
        let tail = after[end + 6..].trim_start();
        if let Some(value) = tail.strip_prefix("<string>")
            && let Some(close) = value.find("</string>")
        {
            out.insert(key, unescape(value[..close].trim()));
        } else if tail.starts_with("<string/>") {
            out.insert(key, String::new());
        }
        rest = &after[end + 6..];
    }
    out
}

fn read_plist(path: &Path) -> Option<HashMap<String, String>> {
    let bytes = std::fs::read(path).ok()?;
    if bytes.starts_with(b"bplist") {
        // Format binaire : converti en XML par l'outil du système (lecture seule).
        let out = std::process::Command::new("/usr/bin/plutil")
            .args(["-convert", "xml1", "-o", "-"])
            .arg(path)
            .output()
            .ok()?;
        return out.status.success().then(|| parse_plist_strings(&String::from_utf8_lossy(&out.stdout)));
    }
    Some(parse_plist_strings(&String::from_utf8_lossy(&bytes)))
}

/// Entrée d'une application, lue dans son `Contents/Info.plist`.
pub fn entry_of_app(app: &Path) -> Option<UninstallEntry> {
    let info = read_plist(&app.join("Contents").join("Info.plist"))?;
    let non_empty = |k: &str| info.get(k).map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
    let bundle_id = non_empty("CFBundleIdentifier")?;
    let exe = non_empty("CFBundleExecutable")?;
    // Un nom d'exécutable ne contient jamais de séparateur : sinon l'application est ignorée.
    if exe.contains('/') || exe == ".." {
        return None;
    }
    let name = non_empty("CFBundleDisplayName")
        .or_else(|| non_empty("CFBundleName"))
        .or_else(|| app.file_stem().map(|s| s.to_string_lossy().into_owned()));
    Some(UninstallEntry {
        key: key_for(&bundle_id),
        display_name: name,
        display_version: non_empty("CFBundleShortVersionString"),
        install_location: Some(app.to_string_lossy().into_owned()),
        uninstall_string: None,
        display_icon: Some(app.join("Contents").join("MacOS").join(exe).to_string_lossy().into_owned()),
    })
}

/// Applications de l'utilisateur (`~/Applications`), lues sans rien lancer.
pub struct MacApps {
    pub dir: PathBuf,
}

impl MacApps {
    pub fn user() -> Self {
        Self { dir: user_applications_dir().unwrap_or_default() }
    }
}

fn is_app_dir(p: &Path) -> bool {
    p.extension().is_some_and(|e| e.eq_ignore_ascii_case("app")) && p.is_dir()
}

impl RegistrySource for MacApps {
    fn entries(&self) -> Vec<UninstallEntry> {
        let Ok(read) = std::fs::read_dir(&self.dir) else { return Vec::new() };
        let mut entries: Vec<UninstallEntry> =
            read.filter_map(Result::ok).map(|e| e.path()).filter(|p| is_app_dir(p)).filter_map(|p| entry_of_app(&p)).collect();
        entries.sort_by(|a, b| a.install_location.cmp(&b.install_location));
        entries
    }
}

// ---------- Image disque ----------

/// Une image disque `.dmg` se termine par un bloc de 512 octets qui commence par « koly ».
pub fn is_dmg(path: &Path) -> io::Result<bool> {
    let mut f = File::open(path)?;
    let len = f.metadata()?.len();
    if len < 512 {
        return Ok(false);
    }
    f.seek(SeekFrom::Start(len - 512))?;
    let mut magic = [0u8; 4];
    f.read_exact(&mut magic)?;
    Ok(&magic == b"koly")
}

/// Unique application à la racine d'un dossier (image montée).
pub fn single_app_in(dir: &Path) -> Result<PathBuf, String> {
    let apps: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| format!("image illisible : {e}"))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| is_app_dir(p) && !p.is_symlink())
        .collect();
    match apps.as_slice() {
        [one] => Ok(one.clone()),
        [] => Err("aucune application dans l'image disque".into()),
        _ => Err("plusieurs applications dans l'image disque".into()),
    }
}

/// Image montée, démontée à la fin (même en cas d'erreur).
#[cfg(target_os = "macos")]
struct Mounted(PathBuf);

#[cfg(target_os = "macos")]
impl Drop for Mounted {
    fn drop(&mut self) {
        let detached = std::process::Command::new("/usr/bin/hdiutil").arg("detach").arg(&self.0).arg("-quiet").status();
        if !detached.is_ok_and(|s| s.success()) {
            let _ = std::process::Command::new("/usr/bin/hdiutil").arg("detach").arg(&self.0).args(["-force", "-quiet"]).status();
        }
        let _ = std::fs::remove_dir(&self.0);
    }
}

#[cfg(target_os = "macos")]
fn run(cmd: &mut std::process::Command, what: &str) -> Result<(), String> {
    let out = cmd.output().map_err(|e| format!("{what} impossible : {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        Err(format!("{what} en échec : {}", err.trim()))
    }
}

/// Installe l'application de l'image `dmg` dans `apps_dir`. `bundle_id` : identifiant attendu
/// (une image qui contient une autre application est refusée). Rend le chemin de l'application.
#[cfg(target_os = "macos")]
pub fn install_dmg(dmg: &Path, apps_dir: &Path, work_dir: &Path, bundle_id: Option<&str>) -> Result<PathBuf, String> {
    use std::process::Command;
    std::fs::create_dir_all(apps_dir).map_err(|e| format!("dossier Applications impossible à créer : {e}"))?;
    let mount = work_dir.join("montage");
    let _ = std::fs::remove_dir(&mount);
    std::fs::create_dir_all(&mount).map_err(|e| format!("dossier de montage impossible à créer : {e}"))?;
    run(
        Command::new("/usr/bin/hdiutil")
            .arg("attach")
            .args(["-nobrowse", "-readonly", "-noautoopen", "-noverify", "-quiet", "-mountpoint"])
            .arg(&mount)
            .arg(dmg),
        "montage de l'image disque",
    )?;
    let mounted = Mounted(mount);
    let source = single_app_in(&mounted.0)?;
    let entry = entry_of_app(&source).ok_or("application de l'image disque illisible")?;
    if let Some(expected) = bundle_id
        && entry.key != key_for(expected)
    {
        return Err(format!("l'image disque contient une autre application ({})", entry.key.trim_start_matches(KEY_PREFIX)));
    }
    let name = source.file_name().ok_or("nom d'application invalide")?.to_owned();
    let target = apps_dir.join(&name);
    // Copie à côté, puis échange : une copie ratée ne casse pas la version installée.
    let staging = apps_dir.join(format!(".{}.bworkshop-tmp", name.to_string_lossy()));
    let _ = std::fs::remove_dir_all(&staging);
    if let Err(e) = run(Command::new("/usr/bin/ditto").arg(&source).arg(&staging), "copie de l'application") {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(e);
    }
    drop(mounted);
    // Fichier téléchargé par le launcher : pas de marque de quarantaine, on la retire au cas où.
    let _ = Command::new("/usr/bin/xattr").args(["-dr", "com.apple.quarantine"]).arg(&staging).status();
    if target.exists() {
        let old = apps_dir.join(format!(".{}.bworkshop-old", name.to_string_lossy()));
        let _ = std::fs::remove_dir_all(&old);
        std::fs::rename(&target, &old).map_err(|e| {
            let _ = std::fs::remove_dir_all(&staging);
            format!("ancienne version impossible à remplacer (est-elle ouverte ?) : {e}")
        })?;
        if let Err(e) = std::fs::rename(&staging, &target) {
            let _ = std::fs::rename(&old, &target);
            let _ = std::fs::remove_dir_all(&staging);
            return Err(format!("installation impossible : {e}"));
        }
        let _ = std::fs::remove_dir_all(&old);
    } else {
        std::fs::rename(&staging, &target).map_err(|e| {
            let _ = std::fs::remove_dir_all(&staging);
            format!("installation impossible : {e}")
        })?;
    }
    Ok(target)
}

/// Met l'application à la corbeille (Finder), sinon la supprime. Seulement un `.app`
/// rangé directement dans `apps_dir`.
#[cfg(target_os = "macos")]
pub fn remove_app(entry: &UninstallEntry, apps_dir: &Path) -> Result<(), String> {
    let app = PathBuf::from(entry.install_location.as_deref().ok_or("emplacement inconnu")?);
    if !is_app_dir(&app) || app.parent() != Some(apps_dir) {
        return Err("cette application n'a pas été installée par le launcher : supprime-la depuis le Finder".into());
    }
    let to_trash = format!(
        "tell application \"Finder\" to delete POSIX file \"{}\"",
        app.to_string_lossy().replace('\\', "\\\\").replace('"', "\\\"")
    );
    let trashed = std::process::Command::new("/usr/bin/osascript").args(["-e", &to_trash]).output();
    if trashed.is_ok_and(|o| o.status.success()) && !app.exists() {
        return Ok(());
    }
    std::fs::remove_dir_all(&app).map_err(|e| format!("suppression impossible : {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLIST: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
  <key>CFBundleDisplayName</key><string>Harebourg Overlay</string>
  <key>CFBundleExecutable</key>
  <string>Harebourg Overlay</string>
  <key>CFBundleIdentifier</key><string>app.bworkshop.harebourg-overlay</string>
  <key>CFBundleShortVersionString</key><string>0.3.0</string>
  <key>LSUIElement</key><false/>
  <key>NSHumanReadableCopyright</key><string>A &amp; B</string>
</dict></plist>"#;

    #[test]
    fn lecture_du_plist() {
        let m = parse_plist_strings(PLIST);
        assert_eq!(m["CFBundleIdentifier"], "app.bworkshop.harebourg-overlay");
        assert_eq!(m["CFBundleExecutable"], "Harebourg Overlay");
        assert_eq!(m["NSHumanReadableCopyright"], "A & B");
        assert!(!m.contains_key("LSUIElement"));
    }

    fn fake_app(dir: &Path, name: &str, plist: &str) -> PathBuf {
        let app = dir.join(format!("{name}.app"));
        std::fs::create_dir_all(app.join("Contents").join("MacOS")).unwrap();
        std::fs::write(app.join("Contents").join("Info.plist"), plist).unwrap();
        app
    }

    #[test]
    fn applications_de_l_utilisateur() {
        let dir = tempfile::tempdir().unwrap();
        let app = fake_app(dir.path(), "Harebourg Overlay", PLIST);
        fake_app(dir.path(), "Cassée", "<plist><dict></dict></plist>");
        std::fs::write(dir.path().join("notes.txt"), "x").unwrap();
        let entries = MacApps { dir: dir.path().to_path_buf() }.entries();
        assert_eq!(entries.len(), 1);
        let e = &entries[0];
        assert_eq!(e.key, "mac:app.bworkshop.harebourg-overlay");
        assert_eq!(e.display_name.as_deref(), Some("Harebourg Overlay"));
        assert_eq!(e.display_version.as_deref(), Some("0.3.0"));
        assert_eq!(e.display_icon.as_deref().map(PathBuf::from), Some(app.join("Contents/MacOS/Harebourg Overlay")));
        assert_eq!(single_app_in(dir.path()).err().as_deref(), Some("plusieurs applications dans l'image disque"));
    }

    #[test]
    fn executable_hors_du_paquet_refuse() {
        let dir = tempfile::tempdir().unwrap();
        let plist = PLIST.replace("<string>Harebourg Overlay</string>\n  <key>CFBundleIdentifier", "<string>../../evil</string>\n  <key>CFBundleIdentifier");
        let app = fake_app(dir.path(), "X", &plist);
        assert!(entry_of_app(&app).is_none());
    }

    #[test]
    fn signature_dmg() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.dmg");
        let mut bytes = vec![0u8; 2048];
        bytes[1536..1540].copy_from_slice(b"koly");
        std::fs::write(&p, &bytes).unwrap();
        assert!(is_dmg(&p).unwrap());
        std::fs::write(&p, b"MZ pas une image").unwrap();
        assert!(!is_dmg(&p).unwrap());
    }

    #[test]
    fn identifiants_connus() {
        assert_eq!(bundle_id_for("grimoire"), Some("app.bworkshop.grimoire"));
        assert_eq!(bundle_id_for("inconnu"), None);
    }
}
