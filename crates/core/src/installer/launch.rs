//! Lancement d'un outil installé : `InstallLocation` + exécutable principal.

use super::registry::UninstallEntry;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

/// `DisplayIcon` vaut souvent `"C:\…\Outil.exe"` ou `C:\…\Outil.exe,0`.
fn icon_path(raw: &str) -> Option<PathBuf> {
    let s = raw.trim().trim_matches('"');
    let s = match s.rfind(',') {
        Some(i) if s[i + 1..].trim().parse::<i32>().is_ok() => &s[..i],
        _ => s,
    };
    let s = s.trim().trim_matches('"');
    s.to_ascii_lowercase().ends_with(".exe").then(|| PathBuf::from(s))
}

fn is_uninstaller(p: &Path) -> bool {
    p.file_name()
        .map(|n| n.to_string_lossy().to_ascii_lowercase().starts_with("unins"))
        .unwrap_or(false)
}

/// Exécutable principal : déclaré dans la table, sinon tiré de `DisplayIcon`.
/// Mac : l'exécutable du paquet `.app` (`Contents/MacOS/…`), lu dans son `Info.plist`.
pub fn main_exe(entry: &UninstallEntry, declared: Option<&str>) -> Option<PathBuf> {
    if entry.key.starts_with(super::mac::KEY_PREFIX) {
        return entry.display_icon.as_deref().map(PathBuf::from);
    }
    if let (Some(rel), Some(dir)) = (declared, entry.install_location.as_deref()) {
        return Some(Path::new(dir).join(rel));
    }
    let exe = entry.display_icon.as_deref().and_then(icon_path)?;
    (!is_uninstaller(&exe)).then_some(exe)
}

/// Lance l'outil, avec son dossier comme dossier courant. Ne l'attend pas :
/// le processus démarré est rendu à l'appelant (pour savoir s'il tourne encore).
pub fn launch(exe: &Path) -> io::Result<std::process::Child> {
    command(exe)?.spawn()
}

/// Comme [`launch`], mais le processus démarre **suspendu** (Windows) : l'appelant **doit**
/// le placer dans son groupe de processus puis le reprendre (`running::track` côté
/// application), sinon il reste figé. Ainsi, aucun processus qu'il ouvrirait dès ses
/// premières instructions n'échappe au groupe.
#[cfg(windows)]
pub fn launch_suspended(exe: &Path) -> io::Result<std::process::Child> {
    use std::os::windows::process::CommandExt;
    const CREATE_SUSPENDED: u32 = 0x0000_0004;
    command(exe)?.creation_flags(CREATE_SUSPENDED).spawn()
}

/// Mac : pas de démarrage suspendu ; le processus devient chef de son propre groupe
/// (`setpgid`), l'équivalent du « job » Windows : les processus qu'il ouvre y restent.
#[cfg(unix)]
pub fn launch_suspended(exe: &Path) -> io::Result<std::process::Child> {
    use std::os::unix::process::CommandExt;
    command(exe)?.process_group(0).spawn()
}

fn command(exe: &Path) -> io::Result<Command> {
    if !exe.is_file() {
        return Err(io::Error::new(io::ErrorKind::NotFound, "exécutable introuvable"));
    }
    let mut cmd = Command::new(exe);
    if let Some(dir) = exe.parent() {
        cmd.current_dir(dir);
    }
    Ok(cmd)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(icon: &str) -> UninstallEntry {
        UninstallEntry {
            key: "{X}_is1".into(),
            display_icon: Some(icon.into()),
            install_location: Some(r"C:\Outils\X\".into()),
            ..Default::default()
        }
    }

    #[test]
    fn exe_depuis_display_icon() {
        assert_eq!(main_exe(&entry(r"C:\Outils\X\Outil.exe"), None), Some(PathBuf::from(r"C:\Outils\X\Outil.exe")));
        assert_eq!(main_exe(&entry(r#""C:\Outils\X\Outil.exe",0"#), None), Some(PathBuf::from(r"C:\Outils\X\Outil.exe")));
        assert_eq!(main_exe(&entry(r"C:\Outils\X\icone.ico"), None), None);
        assert_eq!(main_exe(&entry(r"C:\Outils\X\unins000.exe"), None), None);
    }

    #[test]
    fn exe_declare_prioritaire() {
        assert_eq!(
            main_exe(&entry(r"C:\Outils\X\icone.ico"), Some("bin\\Outil.exe")),
            Some(PathBuf::from(r"C:\Outils\X\bin\Outil.exe"))
        );
    }
}
