//! Logiciels lancés **par le launcher** : savoir s'ils tournent, les fermer.
//!
//! Règles (voir « Ce que le launcher ne fait pas » dans le README) :
//! - seuls les processus démarrés par le launcher sont suivis, par le numéro et la
//!   poignée que Windows lui a rendus au lancement ; aucun autre processus n'est concerné
//!   (pas d'`OpenProcess`, jamais sur Dofus ni sur un autre programme) ;
//! - fermer = d'abord envoyer `WM_CLOSE` (comme cliquer sur la croix) aux fenêtres
//!   principales de ce processus seulement. Jamais de clic, jamais de touche ;
//! - le processus lancé est placé dans un groupe (« job ») créé par le launcher : les
//!   processus qu'il ouvre lui-même y entrent, et rien d'autre ;
//! - arrêt forcé (`TerminateJobObject` sur ce groupe) seulement si le logiciel tourne encore
//!   après une fermeture polie demandée il y a entre 3 s et 60 s ; au-delà, un clic redemande
//!   une fermeture polie (le logiciel a pu proposer « Enregistrer ? » et l'utilisateur annuler) ;
//! - le titre des fenêtres ne sert pas à choisir l'outil (un outil peut s'appeler
//!   « Dofus Color Picker ») ; il sert seulement de garde-fou : si un outil a lui-même démarré
//!   l'Ankama Launcher ou Dofus, ces processus entrent dans son groupe. Tout processus du groupe
//!   (autre que celui lancé) qui a une fenêtre au titre de Dofus ou d'Ankama est alors écarté :
//!   il ne reçoit pas `WM_CLOSE`, il n'est pas pris pour un outil, et l'arrêt forcé ne vise plus
//!   le groupe mais seulement le processus lancé.

use serde::Serialize;
use std::collections::BTreeMap;
use std::os::windows::io::AsRawHandle;
use std::process::Child;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

pub const EVENT_UPDATED: &str = "running-updated";
/// Délai entre la fermeture polie et l'arrêt forcé possible.
pub const FORCE_AFTER: Duration = Duration::from_secs(3);
/// Au-delà, la demande de fermeture est oubliée : un clic redemande une fermeture polie.
pub const FORCE_WINDOW: Duration = Duration::from_secs(60);

/// La demande de fermeture permet-elle l'arrêt forcé maintenant ?
fn force_allowed(requested: Option<Instant>) -> bool {
    requested.is_some_and(|t| (FORCE_AFTER..FORCE_WINDOW).contains(&t.elapsed()))
}

/// Titre d'une fenêtre du jeu ou de l'Ankama Launcher (motif des réglages pour Dofus).
fn is_game_title(title: &str, pattern: &str) -> bool {
    bworkshop_core::dofus_tracker::title_matches(title, pattern) || title.to_lowercase().contains("ankama")
}

pub fn game_pattern(app: &AppHandle) -> String {
    app.state::<crate::state::AppState>().settings().dofus_title_pattern
}

struct Entry {
    pid: u32,
    /// Poignée rendue par `CreateProcess` au lancement, valable tant que l'entrée existe
    /// (le fil de suivi retire l'entrée **avant** de libérer le processus).
    handle: isize,
    /// Groupe (« job » Windows) créé par le launcher pour ce lancement : les processus que
    /// le logiciel ouvre lui-même y entrent automatiquement, et rien d'autre.
    job: Option<native::Job>,
    close_requested: Option<Instant>,
    /// Faux tant qu'aucune fenêtre du logiciel n'est apparue (tuile « en démarrage »).
    ready: bool,
}

/// Délai au-delà duquel on cesse d'attendre une fenêtre (logiciel sans fenêtre visible).
const READY_TIMEOUT: Duration = Duration::from_secs(30);
const READY_POLL: Duration = Duration::from_millis(250);

/// Ce que la barre affiche : logiciels lancés, et ceux encore sans fenêtre.
#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunningView {
    pub running: Vec<String>,
    pub starting: Vec<String>,
}

#[derive(Default)]
pub struct Running {
    entries: Mutex<BTreeMap<String, Entry>>,
}

impl Running {
    pub fn view(&self) -> RunningView {
        let Ok(p) = self.entries.lock() else { return RunningView::default() };
        RunningView {
            running: p.keys().cloned().collect(),
            starting: p.iter().filter(|(_, e)| !e.ready).map(|(k, _)| k.clone()).collect(),
        }
    }

    /// Le logiciel a-t-il encore une fenêtre visible ? (Faux s'il s'est caché près de l'horloge.)
    pub fn has_window(&self, id: &str, pattern: &str) -> bool {
        let pid = self.entries.lock().ok().and_then(|p| p.get(id).map(|e| e.pid));
        pid.and_then(|pid| self.pids_of(id, pid, pattern)).is_some_and(|pids| native::has_window(&pids))
    }

    pub fn is_running(&self, id: &str) -> bool {
        self.entries.lock().is_ok_and(|p| p.contains_key(id))
    }

    /// Une fermeture polie a été demandée il y a assez longtemps : l'arrêt forcé est possible.
    pub fn can_force(&self, id: &str) -> bool {
        self.entries.lock().is_ok_and(|p| p.get(id).is_some_and(|e| force_allowed(e.close_requested)))
    }

    /// Ce processus a-t-il été démarré par le launcher (lui ou un processus de son groupe) ?
    /// Sert au suivi de Dofus : un outil lancé peut s'appeler « Dofus … » sans être le jeu.
    /// Un processus du groupe (autre que celui lancé) qui a une fenêtre au titre du jeu n'est
    /// pas compté : c'est le jeu, démarré à travers un outil.
    pub fn owns_pid(&self, pid: u32, pattern: &str) -> bool {
        let Ok(p) = self.entries.lock() else { return false };
        p.values().any(|e| {
            e.pid == pid
                || e.job.as_ref().is_some_and(|j| {
                    native::job_pids(j).contains(&pid) && native::game_pids(&[pid], &|t| is_game_title(t, pattern)).is_empty()
                })
        })
    }

    /// Processus de l'outil (celui lancé et ceux de son groupe, sans le jeu ni l'Ankama
    /// Launcher), si l'entrée `pid` existe toujours.
    fn pids_of(&self, id: &str, pid: u32, pattern: &str) -> Option<Vec<u32>> {
        let p = self.entries.lock().ok()?;
        let e = p.get(id).filter(|e| e.pid == pid)?;
        Some(split_pids(e, pattern).0)
    }

    fn set_ready(&self, id: &str, pid: u32) {
        if let Ok(mut p) = self.entries.lock() {
            if let Some(e) = p.get_mut(id).filter(|e| e.pid == pid) {
                e.ready = true;
            }
        }
    }
}

/// Sépare les processus d'une entrée : ceux de l'outil, et ceux écartés (jeu, Ankama Launcher).
/// Le processus lancé reste toujours du côté de l'outil.
fn split_pids(e: &Entry, pattern: &str) -> (Vec<u32>, Vec<u32>) {
    let mut members = e.job.as_ref().map(native::job_pids).unwrap_or_default();
    members.retain(|&p| p != e.pid);
    let foreign = native::game_pids(&members, &|t| is_game_title(t, pattern));
    let mut tool: Vec<u32> = members.into_iter().filter(|p| !foreign.contains(p)).collect();
    tool.push(e.pid);
    (tool, foreign)
}

fn emit(app: &AppHandle) {
    let _ = app.emit(EVENT_UPDATED, app.state::<Running>().view());
}

/// Attend la première fenêtre du logiciel (vérification légère, 4 fois par seconde,
/// 30 s au plus), puis prévient la barre : la tuile quitte l'état « en démarrage ».
fn watch_ready(app: AppHandle, id: String, pid: u32) {
    let spawned = std::thread::Builder::new().name(format!("demarrage-{id}")).spawn(move || {
        let started = Instant::now();
        let pattern = game_pattern(&app);
        loop {
            std::thread::sleep(READY_POLL);
            let running = app.state::<Running>();
            let Some(pids) = running.pids_of(&id, pid, &pattern) else { return };
            if native::has_window(&pids) || started.elapsed() >= READY_TIMEOUT {
                running.set_ready(&id, pid);
                emit(&app);
                return;
            }
        }
    });
    if let Err(e) = spawned {
        log::warn!("Attente de la fenêtre de {pid} impossible : {e}");
    }
}

/// Suit un processus que le launcher vient de démarrer. Un fil attend sa fin
/// (attente bloquante : aucun calcul pendant ce temps).
/// Refusé si l'outil tourne déjà : l'entrée existante (et son groupe) ne doit pas être perdue.
pub fn track(app: &AppHandle, id: &str, mut child: Child) -> Result<(), String> {
    let pid = child.id();
    let handle = child.as_raw_handle() as isize;
    {
        let running = app.state::<Running>();
        let Ok(mut p) = running.entries.lock() else {
            let _ = child.kill();
            return Err("État indisponible.".into());
        };
        if p.contains_key(id) {
            // Deux lancements presque simultanés : le second est arrêté.
            let _ = child.kill();
            return Err("Cet outil est déjà lancé.".into());
        }
        let job = native::job_for(handle);
        if job.is_none() {
            log::warn!("{id} : groupe de processus indisponible, seul le processus lancé sera visé");
        }
        p.insert(id.to_string(), Entry { pid, handle, job, close_requested: None, ready: false });
    }
    emit(app);
    watch_ready(app.clone(), id.to_string(), pid);
    let (app, id) = (app.clone(), id.to_string());
    let spawned = std::thread::Builder::new().name(format!("suivi-{id}")).spawn(move || {
        let _ = child.wait();
        if let Ok(mut p) = app.state::<Running>().entries.lock() {
            if p.get(&id).is_some_and(|e| e.pid == pid) {
                p.remove(&id);
            }
        }
        // `child` (et sa poignée) n'est libéré qu'ici, après le retrait de l'entrée.
        drop(child);
        log::info!("{id} s'est arrêté");
        emit(&app);
        // Il s'est peut-être mis à jour lui-même : on relit sa version.
        crate::installer_bridge::recheck_after_exit(&app);
    });
    if let Err(e) = spawned {
        log::warn!("Suivi de {pid} impossible : {e}");
    }
    Ok(())
}

/// Demande poliment au logiciel de se fermer (comme sa croix).
pub fn close(app: &AppHandle, id: &str) -> Result<(), String> {
    let running = app.state::<Running>();
    let pattern = game_pattern(app);
    let pids = {
        let mut entries = running.entries.lock().map_err(|_| "État indisponible.")?;
        let e = entries
            .get_mut(id)
            .ok_or("Ce logiciel n'a pas été lancé par le launcher : il ne peut pas le fermer.")?;
        e.close_requested = Some(Instant::now());
        // Le processus lancé et ceux qu'il a lui-même ouverts (même groupe), sans le jeu.
        split_pids(e, &pattern).0
    };
    let closed = native::close_windows_of(&pids);
    log::info!("{id} : fermeture demandée ({closed} fenêtre(s))");
    Ok(())
}

/// Arrête de force un logiciel lancé par le launcher, qui n'a pas répondu à la fermeture polie.
pub fn force_close(app: &AppHandle, id: &str) -> Result<(), String> {
    let running = app.state::<Running>();
    let pattern = game_pattern(app);
    let entries = running.entries.lock().map_err(|_| "État indisponible.")?;
    let e = entries
        .get(id)
        .ok_or("Ce logiciel n'a pas été lancé par le launcher : il ne peut pas le fermer.")?;
    if !force_allowed(e.close_requested) {
        return Err("Demande d'abord une fermeture normale.".into());
    }
    // Verrou tenu pendant l'appel : la poignée ne peut pas être libérée entre-temps.
    // Avec un groupe : tout le groupe (le logiciel et les processus qu'il a ouverts), sauf si
    // le jeu ou l'Ankama Launcher s'y trouve : seul le processus lancé est alors arrêté.
    let foreign = split_pids(e, &pattern).1;
    let result = match &e.job {
        Some(job) if foreign.is_empty() => native::terminate_job(job),
        Some(_) => {
            log::warn!("{id} : le jeu ou l'Ankama Launcher est dans son groupe, seul le processus lancé est arrêté");
            native::terminate(e.handle)
        }
        None => native::terminate(e.handle),
    };
    result.map_err(|err| format!("Arrêt forcé impossible : {err}"))?;
    log::warn!("{id} : arrêt forcé (ne répondait pas à la fermeture)");
    Ok(())
}

mod native {
    use windows::core::BOOL;
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindow, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible, PostMessageW, GW_OWNER,
        WM_CLOSE,
    };

    struct Search<'a> {
        pids: &'a [u32],
        /// Vrai : envoyer `WM_CLOSE`. Faux : seulement compter les fenêtres.
        post: bool,
        closed: u32,
    }

    unsafe extern "system" fn visit(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let search = unsafe { &mut *(lparam.0 as *mut Search) };
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        if !search.pids.contains(&pid) {
            return true.into();
        }
        // Fenêtres principales seulement : visibles et sans propriétaire.
        let owned = unsafe { GetWindow(hwnd, GW_OWNER) }.is_ok_and(|o| !o.is_invalid());
        if !unsafe { IsWindowVisible(hwnd) }.as_bool() || owned {
            return true.into();
        }
        if !search.post {
            search.closed += 1;
            return false.into(); // une fenêtre suffit : on arrête l'énumération
        }
        if unsafe { PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0)) }.is_ok() {
            search.closed += 1;
        }
        true.into()
    }

    fn search(pids: &[u32], post: bool) -> u32 {
        let mut search = Search { pids, post, closed: 0 };
        let _ = unsafe { EnumWindows(Some(visit), LPARAM(&mut search as *mut Search as isize)) };
        search.closed
    }

    /// Envoie `WM_CLOSE` aux fenêtres principales des processus `pids`. Renvoie leur nombre.
    pub fn close_windows_of(pids: &[u32]) -> u32 {
        search(pids, true)
    }

    /// Un des processus `pids` a-t-il une fenêtre principale visible ?
    pub fn has_window(pids: &[u32]) -> bool {
        search(pids, false) > 0
    }

    struct TitleSearch<'a> {
        pids: &'a [u32],
        is_game: &'a dyn Fn(&str) -> bool,
        found: Vec<u32>,
    }

    unsafe extern "system" fn visit_titles(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let search = unsafe { &mut *(lparam.0 as *mut TitleSearch) };
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        if search.pids.contains(&pid) && !search.found.contains(&pid) {
            let mut buf = [0u16; 512];
            let n = unsafe { GetWindowTextW(hwnd, &mut buf) }.max(0) as usize;
            if (search.is_game)(&String::from_utf16_lossy(&buf[..n.min(buf.len())])) {
                search.found.push(pid);
            }
        }
        true.into()
    }

    /// Parmi `pids`, ceux qui ont une fenêtre (visible ou non) au titre du jeu ou de l'Ankama
    /// Launcher. Seuls le titre et le numéro de processus sont lus, sans ouvrir de poignée.
    pub fn game_pids(pids: &[u32], is_game: &dyn Fn(&str) -> bool) -> Vec<u32> {
        if pids.is_empty() {
            return Vec::new();
        }
        let mut search = TitleSearch { pids, is_game, found: Vec::new() };
        let _ = unsafe { EnumWindows(Some(visit_titles), LPARAM(&mut search as *mut TitleSearch as isize)) };
        search.found
    }

    /// Arrête le processus désigné par la poignée reçue à son lancement.
    pub fn terminate(handle: isize) -> windows::core::Result<()> {
        use windows::Win32::Foundation::HANDLE;
        use windows::Win32::System::Threading::TerminateProcess;
        unsafe { TerminateProcess(HANDLE(handle as _), 1) }
    }

    /// Groupe de processus (« job ») créé par le launcher pour un lancement.
    /// Sans `KILL_ON_JOB_CLOSE` : quitter le launcher ne ferme pas les logiciels.
    pub struct Job(windows::Win32::Foundation::HANDLE);

    // La poignée de groupe peut être utilisée depuis n'importe quel fil.
    unsafe impl Send for Job {}

    impl Drop for Job {
        fn drop(&mut self) {
            let _ = unsafe { windows::Win32::Foundation::CloseHandle(self.0) };
        }
    }

    /// Crée un groupe et y place le processus que le launcher vient de lancer (par la poignée
    /// rendue au lancement : aucune autre poignée n'est ouverte). Les processus qu'il ouvrira
    /// lui-même y entreront automatiquement.
    pub fn job_for(process: isize) -> Option<Job> {
        use windows::core::PCWSTR;
        use windows::Win32::Foundation::HANDLE;
        use windows::Win32::System::JobObjects::{AssignProcessToJobObject, CreateJobObjectW};
        let job = Job(unsafe { CreateJobObjectW(None, PCWSTR::null()) }.ok()?);
        unsafe { AssignProcessToJobObject(job.0, HANDLE(process as _)) }.ok()?;
        Some(job)
    }

    /// Numéros des processus encore présents dans le groupe.
    pub fn job_pids(job: &Job) -> Vec<u32> {
        use windows::Win32::System::JobObjects::{JobObjectBasicProcessIdList, QueryInformationJobObject};
        // JOBOBJECT_BASIC_PROCESS_ID_LIST : deux u32 (8 octets) puis un tableau de usize.
        let mut buf = vec![0usize; 1 + 64];
        let ok = unsafe {
            QueryInformationJobObject(
                Some(job.0),
                JobObjectBasicProcessIdList,
                buf.as_mut_ptr().cast(),
                (buf.len() * std::mem::size_of::<usize>()) as u32,
                None,
            )
        };
        if ok.is_err() {
            return Vec::new();
        }
        let header = buf[0] as u64;
        let listed = ((header >> 32) as usize).min(buf.len() - 1);
        buf[1..=listed].iter().map(|&p| p as u32).collect()
    }

    /// Arrête tous les processus du groupe.
    pub fn terminate_job(job: &Job) -> windows::core::Result<()> {
        use windows::Win32::System::JobObjects::TerminateJobObject;
        unsafe { TerminateJobObject(job.0, 1) }
    }
}