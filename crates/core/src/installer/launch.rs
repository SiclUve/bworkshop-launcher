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

#[cfg(target_os = "macos")]
unsafe extern "C" {
    /// Fonction du système (libSystem, macOS 10.14 et plus), utilisée par les terminaux et les
    /// navigateurs : le processus lancé répond **lui-même** de ses autorisations.
    fn responsibility_spawnattrs_setdisclaim(attr: *mut libc::posix_spawnattr_t, disclaim: libc::c_int) -> libc::c_int;
}

/// Mac : lance l'outil et rend son numéro de processus. L'appelant **doit** l'attendre
/// (`waitpid`) pour le libérer à la fin.
/// - Chef de son propre groupe de processus (l'équivalent du « job » Windows : les processus
///   qu'il ouvre y restent).
/// - « Désavoué » par le launcher : macOS demande les autorisations (enregistrement de l'écran,
///   surveillance du clavier…) au nom de l'outil, pas du launcher. Sans cela, un programme lancé
///   directement hérite des autorisations de son parent.
#[cfg(target_os = "macos")]
pub fn spawn_app(exe: &Path) -> io::Result<u32> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    if !exe.is_file() {
        return Err(io::Error::new(io::ErrorKind::NotFound, "exécutable introuvable"));
    }
    let path = CString::new(exe.as_os_str().as_bytes()).map_err(|_| io::Error::other("chemin invalide"))?;
    let argv = [path.as_ptr() as *mut libc::c_char, std::ptr::null_mut()];
    let mut pid: libc::pid_t = 0;
    unsafe {
        let mut attr: libc::posix_spawnattr_t = std::mem::zeroed();
        let rc = libc::posix_spawnattr_init(&mut attr);
        if rc != 0 {
            return Err(io::Error::from_raw_os_error(rc));
        }
        let mut rc = libc::posix_spawnattr_setflags(&mut attr, libc::POSIX_SPAWN_SETPGROUP as libc::c_short);
        if rc == 0 {
            rc = libc::posix_spawnattr_setpgroup(&mut attr, 0);
        }
        if rc == 0 && responsibility_spawnattrs_setdisclaim(&mut attr, 1) != 0 {
            log::warn!("Lancement : autorisations rattachées au launcher (désaveu refusé par macOS)");
        }
        if rc == 0 {
            rc = libc::posix_spawn(&mut pid, path.as_ptr(), std::ptr::null(), &attr, argv.as_ptr(), *libc::_NSGetEnviron());
        }
        libc::posix_spawnattr_destroy(&mut attr);
        if rc != 0 {
            return Err(io::Error::from_raw_os_error(rc));
        }
    }
    Ok(pid as u32)
}

#[cfg_attr(target_os = "macos", allow(dead_code))]
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

    #[cfg_attr(not(windows), allow(dead_code))]
    fn entry(icon: &str) -> UninstallEntry {
        UninstallEntry {
            key: "{X}_is1".into(),
            display_icon: Some(icon.into()),
            install_location: Some(r"C:\Outils\X\".into()),
            ..Default::default()
        }
    }

    #[test]
    #[cfg(windows)]
    fn exe_depuis_display_icon() {
        assert_eq!(main_exe(&entry(r"C:\Outils\X\Outil.exe"), None), Some(PathBuf::from(r"C:\Outils\X\Outil.exe")));
        assert_eq!(main_exe(&entry(r#""C:\Outils\X\Outil.exe",0"#), None), Some(PathBuf::from(r"C:\Outils\X\Outil.exe")));
        assert_eq!(main_exe(&entry(r"C:\Outils\X\icone.ico"), None), None);
        assert_eq!(main_exe(&entry(r"C:\Outils\X\unins000.exe"), None), None);
    }

    #[test]
    #[cfg(windows)]
    fn exe_declare_prioritaire() {
        assert_eq!(
            main_exe(&entry(r"C:\Outils\X\icone.ico"), Some("bin\\Outil.exe")),
            Some(PathBuf::from(r"C:\Outils\X\bin\Outil.exe"))
        );
    }
}
