//! Exécution silencieuse des installateurs et désinstalleurs (Inno Setup ou NSIS).

use super::kind::InstallerKind;
use super::registry::UninstallEntry;
use super::{inno, nsis};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

/// Code Windows « l'opération demande une élévation ».
const ERROR_ELEVATION_REQUIRED: i32 = 740;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RunError {
    #[error("cet installateur demande les droits administrateur : le launcher n'installe que pour l'utilisateur")]
    ElevationRequired,
    #[error("{kind} s'est arrêté (code {code} : {meaning})")]
    ExitCode { kind: &'static str, code: i32, meaning: &'static str },
    #[error("impossible de lancer : {0}")]
    Spawn(String),
    #[error("commande de désinstallation illisible")]
    BadUninstallString,
    #[error("désinstallation automatique impossible pour cet outil : utilise « Applications installées » dans les paramètres de Windows")]
    UnsupportedUninstaller,
    #[error("la désinstallation n'est pas confirmée après {0} s")]
    UninstallTimeout(u64),
}

fn args_for(kind: InstallerKind, uninstall: bool) -> &'static [&'static str] {
    match (kind, uninstall) {
        (InstallerKind::Inno, false) => inno::SILENT_INSTALL_ARGS,
        (InstallerKind::Inno, true) => inno::SILENT_UNINSTALL_ARGS,
        (InstallerKind::Nsis, false) => nsis::SILENT_INSTALL_ARGS,
        (InstallerKind::Nsis, true) => nsis::SILENT_UNINSTALL_ARGS,
    }
}

fn meaning(kind: InstallerKind, code: i32) -> &'static str {
    match kind {
        InstallerKind::Inno => inno::exit_code_meaning(code),
        InstallerKind::Nsis => nsis::exit_code_meaning(code),
    }
}

fn run(kind: InstallerKind, exe: &Path, args: &[String]) -> Result<(), RunError> {
    let status = Command::new(exe).args(args).status().map_err(|e| {
        if e.raw_os_error() == Some(ERROR_ELEVATION_REQUIRED) {
            RunError::ElevationRequired
        } else {
            RunError::Spawn(e.to_string())
        }
    })?;
    match status.code() {
        Some(0) => Ok(()),
        Some(code) => Err(RunError::ExitCode { kind: kind.label(), code, meaning: meaning(kind, code) }),
        None => Err(RunError::ExitCode { kind: kind.label(), code: -1, meaning: "arrêt anormal" }),
    }
}

/// Lance l'installateur en mode silencieux et attend la fin.
/// (Inno et NSIS rendent la main seulement quand l'installation est terminée.)
pub fn install(kind: InstallerKind, setup: &Path) -> Result<(), RunError> {
    let args: Vec<String> = args_for(kind, false).iter().map(|s| s.to_string()).collect();
    log::info!("Installation silencieuse ({}) : {}", kind.label(), setup.display());
    run(kind, setup, &args)
}

/// Découpe une `UninstallString` : `"C:\…\unins000.exe" /ARG` → (exe, [args]).
pub fn parse_command_line(s: &str) -> Option<(PathBuf, Vec<String>)> {
    let s = s.trim();
    let (exe, rest) = if let Some(stripped) = s.strip_prefix('"') {
        let end = stripped.find('"')?;
        (&stripped[..end], &stripped[end + 1..])
    } else {
        let lower = s.to_ascii_lowercase();
        let end = lower.find(".exe")? + 4;
        (&s[..end], &s[end..])
    };
    if !exe.to_ascii_lowercase().ends_with(".exe") {
        return None;
    }
    let args = rest.split_whitespace().map(str::to_string).collect();
    Some((PathBuf::from(exe), args))
}

/// Type du désinstalleur d'une entrée : clé `_is1` → Inno ; sinon on lit la
/// signature du désinstalleur lui-même. Aucun argument « silencieux » n'est
/// jamais envoyé à un désinstalleur d'un autre type.
pub fn uninstaller_kind(entry: &UninstallEntry, exe: &Path) -> Option<InstallerKind> {
    if entry.is_inno() {
        return Some(InstallerKind::Inno);
    }
    nsis::is_nsis(exe).ok().filter(|b| *b).map(|_| InstallerKind::Nsis)
}

/// Désinstalle silencieusement, puis attend que `is_gone()` le confirme.
///
/// Les désinstalleurs Inno et NSIS se recopient dans un dossier temporaire et
/// rendent la main tout de suite : on vérifie donc la disparition de l'entrée du
/// registre, avec un délai maximal (attente bornée, pas une boucle permanente).
pub fn uninstall(entry: &UninstallEntry, timeout: Duration, is_gone: impl Fn() -> bool) -> Result<(), RunError> {
    let raw = entry.uninstall_string.as_deref().ok_or(RunError::BadUninstallString)?;
    let (exe, mut args) = parse_command_line(raw).ok_or(RunError::BadUninstallString)?;
    let kind = uninstaller_kind(entry, &exe).ok_or(RunError::UnsupportedUninstaller)?;
    for a in args_for(kind, true) {
        if !args.iter().any(|x| x.eq_ignore_ascii_case(a)) {
            args.push(a.to_string());
        }
    }
    log::info!("Désinstallation silencieuse ({}) : {}", kind.label(), exe.display());
    run(kind, &exe, &args)?;
    let start = Instant::now();
    while start.elapsed() < timeout {
        if is_gone() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    if is_gone() { Ok(()) } else { Err(RunError::UninstallTimeout(timeout.as_secs())) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoupage_de_la_commande() {
        let (exe, args) = parse_command_line(r#""C:\Program Files\X\unins000.exe" /LOG"#).unwrap();
        assert_eq!(exe, PathBuf::from(r"C:\Program Files\X\unins000.exe"));
        assert_eq!(args, vec!["/LOG"]);
        let (exe, args) = parse_command_line(r"C:\X\uninstall.exe").unwrap();
        assert_eq!(exe, PathBuf::from(r"C:\X\uninstall.exe"));
        assert!(args.is_empty());
        assert!(parse_command_line(r#""C:\X\script.bat""#).is_none());
        assert!(parse_command_line(r#"MsiExec.exe /X{GUID}"#).is_some_and(|(e, _)| e.ends_with("MsiExec.exe")));
        assert!(parse_command_line("").is_none());
    }

    #[test]
    fn desinstalleur_inconnu_refuse() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("autre.exe");
        std::fs::write(&exe, b"MZ un autre desinstalleur").unwrap();
        let entry = UninstallEntry {
            key: "AutreOutil".into(),
            uninstall_string: Some(format!("\"{}\"", exe.display())),
            ..Default::default()
        };
        assert_eq!(uninstall(&entry, Duration::from_secs(1), || true), Err(RunError::UnsupportedUninstaller));
        // Une clé _is1 est toujours traitée comme Inno.
        let inno = UninstallEntry { key: "{X}_is1".into(), ..Default::default() };
        assert_eq!(uninstaller_kind(&inno, &exe), Some(InstallerKind::Inno));
    }
}
