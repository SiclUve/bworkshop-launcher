//! Opérations d'installation en arrière-plan, avec progression envoyée à l'interface.
//!
//! États (partie 5.1) : Non installé → Téléchargement → Vérification → Installation
//! → Installé ; Installé → Mise à jour disponible → Téléchargement → … ; chaque étape
//! peut finir en Erreur (bouton « Réessayer ») ; un téléchargement peut être annulé.
//! Les opérations n'ouvrent jamais de fenêtre et ne prennent jamais le focus.

use crate::state::AppState;
use bworkshop_core::catalog::Tool;
use bworkshop_core::installer::download::{self, DownloadError, DownloadRequest, Progress};
use bworkshop_core::installer::registry::{RegistrySource, UninstallEntry, WindowsRegistry};
use bworkshop_core::installer::status::{entry_for, status_of, InstallStatus};
use bworkshop_core::installer::version::is_newer;
use bworkshop_core::installer::{kind, known_apps, launch, max_download_size, runner};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

pub const EVENT_PROGRESS: &str = "install-progress";
pub const EVENT_INSTALLS: &str = "installs-updated";
const UNINSTALL_TIMEOUT: Duration = Duration::from_secs(120);
const PROGRESS_INTERVAL: Duration = Duration::from_millis(200);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    Downloading,
    Verifying,
    Installing,
    Uninstalling,
    /// Fichier qui n'est pas un installateur Inno : téléchargé, dossier ouvert.
    Downloaded,
    Done,
    Cancelled,
    Error,
}

impl Phase {
    fn is_final(self) -> bool {
        matches!(self, Phase::Downloaded | Phase::Done | Phase::Cancelled | Phase::Error)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressEvent {
    pub id: String,
    /// « install » ou « uninstall » : ce que « Réessayer » doit relancer.
    pub operation: &'static str,
    pub phase: Phase,
    pub received: u64,
    pub total: Option<u64>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallsView {
    pub statuses: HashMap<String, InstallStatus>,
    /// Opérations en cours ou dernière erreur, par outil.
    pub activity: HashMap<String, ProgressEvent>,
}

#[derive(Default)]
pub struct InstallerBridge {
    /// Drapeau d'annulation de chaque opération en cours.
    jobs: Mutex<HashMap<String, Arc<AtomicBool>>>,
    /// Dernier événement par outil (en cours, ou erreur à afficher).
    activity: Mutex<HashMap<String, ProgressEvent>>,
    /// Un seul installateur ou désinstalleur à la fois.
    installer_lock: Mutex<()>,
}

fn bridge(app: &AppHandle) -> Arc<InstallerBridge> {
    app.state::<Arc<InstallerBridge>>().inner().clone()
}

fn emit(app: &AppHandle, ev: ProgressEvent) {
    let b = bridge(app);
    if let Ok(mut a) = b.activity.lock() {
        if matches!(ev.phase, Phase::Done | Phase::Cancelled | Phase::Downloaded) {
            a.remove(&ev.id);
        } else {
            a.insert(ev.id.clone(), ev.clone());
        }
    }
    if ev.phase.is_final() {
        if let Ok(mut j) = b.jobs.lock() {
            j.remove(&ev.id);
        }
    }
    let _ = app.emit(EVENT_PROGRESS, &ev);
}

fn emit_phase(app: &AppHandle, operation: &'static str, id: &str, phase: Phase, message: Option<String>) {
    emit(app, ProgressEvent { id: id.into(), operation, phase, received: 0, total: None, message });
}

fn tool_by_id(app: &AppHandle, id: &str) -> Result<Tool, String> {
    app.state::<AppState>()
        .catalog()
        .and_then(|c| c.tools.into_iter().find(|t| t.id == id))
        .ok_or_else(|| "Outil inconnu du catalogue".to_string())
}

fn registry_entries() -> Vec<UninstallEntry> {
    WindowsRegistry.entries()
}

/// États de tous les outils du catalogue + opérations en cours.
pub fn view(app: &AppHandle) -> InstallsView {
    let entries = registry_entries();
    let statuses = app
        .state::<AppState>()
        .catalog()
        .map(|c| {
            c.tools
                .iter()
                .map(|t| (t.id.clone(), status_of(entry_for(&entries, t), t.version.as_deref())))
                .collect()
        })
        .unwrap_or_default();
    let activity = bridge(app).activity.lock().map(|a| a.clone()).unwrap_or_default();
    InstallsView { statuses, activity }
}

fn notify_installs(app: &AppHandle) {
    let _ = app.emit(EVENT_INSTALLS, view(app));
}

/// Relectures des versions installées après la fin d'un outil lancé par le launcher :
/// tout de suite, puis un peu plus tard (son installeur de mise à jour peut encore tourner).
const RECHECK_AFTER_EXIT: [std::time::Duration; 3] = [
    std::time::Duration::ZERO,
    std::time::Duration::from_secs(30),
    std::time::Duration::from_secs(120),
];

/// Un outil a pu se mettre à jour lui-même : relit Windows (lecture seule du registre)
/// et prévient la fenêtre principale et la barre. Bloquant : à appeler depuis un fil à part.
pub fn recheck_after_exit(app: &AppHandle) {
    let mut last = None;
    for delay in RECHECK_AFTER_EXIT {
        std::thread::sleep(delay);
        let current = view(app).statuses;
        if last.as_ref() != Some(&current) {
            notify_installs(app);
            last = Some(current);
        }
    }
}

/// Démarre un fil d'opération pour un outil. Refuse s'il y en a déjà une.
fn start_job(app: &AppHandle, id: &str, work: impl FnOnce(AppHandle, Arc<AtomicBool>) + Send + 'static) -> Result<(), String> {
    let b = bridge(app);
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut jobs = b.jobs.lock().map_err(|_| "état interne indisponible")?;
        if jobs.contains_key(id) {
            return Err("Une opération est déjà en cours pour cet outil".into());
        }
        jobs.insert(id.to_string(), cancel.clone());
    }
    let app = app.clone();
    let id_owned = id.to_string();
    std::thread::Builder::new()
        .name(format!("installation {id}"))
        .spawn(move || work(app, cancel))
        .map(|_| ())
        .map_err(|e| {
            if let Ok(mut j) = b.jobs.lock() {
                j.remove(&id_owned);
            }
            format!("Impossible de démarrer l'opération : {e}")
        })
}

/// Sans empreinte SHA-256 publiée dans le catalogue, rien n'est téléchargé ni lancé :
/// impossible de vérifier que le fichier reçu est celui annoncé.
pub fn install(app: &AppHandle, id: &str) -> Result<(), String> {
    let tool = tool_by_id(app, id)?;
    let dl = tool
        .download
        .clone()
        .filter(|_| tool.available)
        .ok_or("Cet outil n'est pas encore téléchargeable")?;
    if dl.sha256.is_none() {
        log::warn!("Installation refusée, pas d'empreinte publiée : {}", tool.id);
        return Err("Cet outil n'a pas d'empreinte SHA-256 dans le catalogue : installation refusée par sécurité.".into());
    }
    start_job(app, id, move |app, cancel| run_install(&app, &tool, &cancel))
}

fn run_install(app: &AppHandle, tool: &Tool, cancel: &AtomicBool) {
    let id = tool.id.as_str();
    let Some(dl) = tool.download.as_ref() else { return };
    let Some(expected) = dl.sha256.clone() else { return };
    // Un dossier par outil (l'identifiant n'a que a-z, 0-9 et « - »), vidé avant chaque
    // téléchargement : rien d'autre que l'installateur vérifié ne s'y trouve.
    let dir = app.state::<AppState>().paths.downloads_dir().join(id);
    let req = DownloadRequest {
        url: dl.url.clone(),
        dest: dir.join(download::file_name_for(&dl.url, id)),
        expected_sha256: Some(expected.clone()),
        max_bytes: max_download_size(dl.size_bytes),
    };
    if let Err(e) = download::clean_tool_dir(&dir, &req.dest) {
        log::warn!("Dossier de téléchargement de {id} impossible à vider : {e}");
        return emit_phase(app, "install", id, Phase::Error, Some(format!("dossier de téléchargement impossible à vider : {e}")));
    }
    log::info!("Téléchargement de {id} ({} octets annoncés)", dl.size_bytes.map(|s| s.to_string()).unwrap_or("?".into()));

    // 1. Téléchargement (progression limitée à 5 événements par seconde).
    let client = match download::download_client() {
        Ok(c) => c,
        Err(e) => return emit_phase(app, "install", id, Phase::Error, Some(e.to_string())),
    };
    let mut last = Instant::now() - PROGRESS_INTERVAL;
    let result = download::download(&client, &req, cancel, |p: Progress| {
        if last.elapsed() >= PROGRESS_INTERVAL || Some(p.received) == p.total {
            last = Instant::now();
            emit(app, ProgressEvent { id: id.into(), operation: "install", phase: Phase::Downloading, received: p.received, total: p.total, message: None });
        }
    });
    let file = match result {
        Ok(d) => d.path,
        Err(DownloadError::Cancelled) => {
            log::info!("Téléchargement de {id} annulé");
            return emit_phase(app, "install", id, Phase::Cancelled, None);
        }
        Err(e) => {
            log::warn!("Téléchargement de {id} en échec : {e}");
            return emit_phase(app, "install", id, Phase::Error, Some(e.to_string()));
        }
    };

    // 2. Vérification : l'empreinte a été contrôlée pendant le téléchargement ;
    //    on reconnaît ici le type d'installateur par sa signature (Inno ou NSIS).
    emit_phase(app, "install", id, Phase::Verifying, None);
    let Some(installer_kind) = kind::detect(&file).ok().flatten() else {
        log::info!("{id} : ni Inno ni NSIS, dossier ouvert sans installation");
        reveal(app, &file);
        return emit_phase(app, "install", id, Phase::Downloaded, None);
    };
    // Un installateur se lance toujours sous un nom en .exe.
    let file = if file.extension().is_some_and(|e| e.eq_ignore_ascii_case("exe")) {
        file
    } else {
        let renamed = dir.join(format!("{id}-setup.exe"));
        if let Err(e) = std::fs::rename(&file, &renamed) {
            let _ = std::fs::remove_file(&file);
            return emit_phase(app, "install", id, Phase::Error, Some(format!("renommage impossible : {e}")));
        }
        renamed
    };
    // Empreinte recalculée juste avant le lancement, fichier verrouillé en écriture
    // jusqu'à la fin de l'installateur.
    let locked = match download::open_verified(&file, &expected) {
        Ok(f) => f,
        Err(e) => return emit_phase(app, "install", id, Phase::Error, Some(e.to_string())),
    };

    // 3. Installation silencieuse (une à la fois).
    emit_phase(app, "install", id, Phase::Installing, None);
    let installed = {
        let b = bridge(app);
        let _guard = b.installer_lock.lock();
        runner::install(installer_kind, &file)
    };
    drop(locked);
    let _ = std::fs::remove_file(&file);
    if let Err(e) = installed {
        log::warn!("Installation de {id} en échec : {e}");
        notify_installs(app);
        return emit_phase(app, "install", id, Phase::Error, Some(e.to_string()));
    }

    // 4. Confirmation dans le registre.
    let entries = registry_entries();
    let outcome = match entry_for(&entries, tool) {
        None => Err("l'installation n'apparaît pas dans le registre".to_string()),
        Some(e) => match (e.display_version.as_deref(), tool.version.as_deref()) {
            (Some(i), Some(a)) if is_newer(a, i) => Err(format!("version installée {i} au lieu de {a}")),
            _ => Ok(()),
        },
    };
    notify_installs(app);
    match outcome {
        Ok(()) => {
            log::info!("{id} installé");
            emit_phase(app, "install", id, Phase::Done, None)
        }
        Err(m) => {
            log::warn!("{id} : {m}");
            emit_phase(app, "install", id, Phase::Error, Some(m))
        }
    }
}

pub fn cancel(app: &AppHandle, id: &str) {
    if let Some(flag) = bridge(app).jobs.lock().ok().and_then(|j| j.get(id).cloned()) {
        flag.store(true, Ordering::SeqCst);
    }
}

/// Efface une erreur affichée (avant « Réessayer » ou sur fermeture du message).
pub fn dismiss(app: &AppHandle, id: &str) {
    if let Ok(mut a) = bridge(app).activity.lock() {
        if a.get(id).is_some_and(|e| e.phase == Phase::Error) {
            a.remove(id);
        }
    }
}

pub fn uninstall(app: &AppHandle, id: &str) -> Result<(), String> {
    let tool = tool_by_id(app, id)?;
    let entries = registry_entries();
    let entry = entry_for(&entries, &tool).cloned().ok_or("Outil introuvable dans le registre")?;
    start_job(app, id, move |app, _cancel| {
        let id = tool.id.as_str();
        emit_phase(&app, "uninstall", id, Phase::Uninstalling, None);
        let result = {
            let b = bridge(&app);
            let _guard = b.installer_lock.lock();
            runner::uninstall(&entry, UNINSTALL_TIMEOUT, || entry_for(&registry_entries(), &tool).is_none())
        };
        notify_installs(&app);
        match result {
            Ok(()) => {
                log::info!("{id} désinstallé");
                emit_phase(&app, "uninstall", id, Phase::Done, None)
            }
            Err(e) => {
                log::warn!("Désinstallation de {id} en échec : {e}");
                emit_phase(&app, "uninstall", id, Phase::Error, Some(e.to_string()))
            }
        }
    })
}

pub fn launch_tool(app: &AppHandle, id: &str) -> Result<(), String> {
    let tool = tool_by_id(app, id)?;
    let entries = registry_entries();
    let entry = entry_for(&entries, &tool).ok_or("Outil non installé")?;
    let declared = known_apps::find(id).and_then(|a| a.main_exe);
    let exe: PathBuf = launch::main_exe(entry, declared).ok_or("Exécutable principal inconnu")?;
    let child = launch::launch(&exe).map_err(|e| format!("Impossible de lancer l'outil : {e}"))?;
    crate::running::track(app, id, child);
    log::info!("{id} lancé");
    Ok(())
}

/// Outil installé (présent dans la liste « Applications » de Windows).
pub fn is_installed(app: &AppHandle, id: &str) -> bool {
    tool_by_id(app, id).is_ok_and(|t| entry_for(&registry_entries(), &t).is_some())
}

/// Outil installé dont une version plus récente est publiée.
pub fn has_update(app: &AppHandle, id: &str) -> bool {
    let Ok(tool) = tool_by_id(app, id) else { return false };
    let entries = registry_entries();
    matches!(status_of(entry_for(&entries, &tool), tool.version.as_deref()), InstallStatus::UpdateAvailable { .. })
}

/// Ouvre l'Explorateur sur l'exécutable de l'outil installé.
pub fn open_folder(app: &AppHandle, id: &str) -> Result<(), String> {
    let tool = tool_by_id(app, id)?;
    let entries = registry_entries();
    let entry = entry_for(&entries, &tool).ok_or("Outil non installé")?;
    let declared = known_apps::find(id).and_then(|a| a.main_exe);
    let exe = launch::main_exe(entry, declared).ok_or("Exécutable principal inconnu")?;
    reveal(app, &exe);
    Ok(())
}

/// Ouvre l'Explorateur sur le fichier téléchargé, sélectionné.
/// `explorer.exe` lit sa ligne de commande à sa façon : l'argument est passé tel quel.
fn reveal(_app: &AppHandle, file: &Path) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let result = std::process::Command::new("explorer.exe")
            .raw_arg(format!("/select,\"{}\"", file.display()))
            .spawn();
        if let Err(e) = result {
            log::warn!("Impossible d'ouvrir le dossier de téléchargement : {e}");
        }
    }
}
