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
//! - arrêt forcé (`TerminateJobObject` sur ce groupe) seulement
//!   si le logiciel tourne encore après une fermeture polie demandée il y a 3 s au moins ;
//! - le titre des fenêtres n'est pas lu : un outil peut s'appeler « Dofus Color Picker ».
//!   La cible est garantie par le groupe créé au lancement, où Dofus n'entre jamais.

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
    pub fn has_window(&self, id: &str) -> bool {
        let pid = self.entries.lock().ok().and_then(|p| p.get(id).map(|e| e.pid));
        pid.and_then(|pid| self.pids_of(id, pid)).is_some_and(|pids| native::has_window(&pids))
    }

    pub fn is_running(&self, id: &str) -> bool {
        self.entries.lock().is_ok_and(|p| p.contains_key(id))
    }

    /// Une fermeture polie a été demandée il y a assez longtemps : l'arrêt forcé est possible.
    pub fn can_force(&self, id: &str) -> bool {
        self.entries
            .lock()
            .is_ok_and(|p| p.get(id).and_then(|e| e.close_requested).is_some_and(|t| t.elapsed() >= FORCE_AFTER))
    }

    /// Ce processus a-t-il été démarré par le launcher (lui ou un processus de son groupe) ?
    /// Sert au suivi de Dofus : un outil lancé peut s'appeler « Dofus … » sans être le jeu.
    pub fn owns_pid(&self, pid: u32) -> bool {
        let Ok(p) = self.entries.lock() else { return false };
        p.values().any(|e| e.pid == pid || e.job.as_ref().is_some_and(|j| native::job_pids(j).contains(&pid)))
    }

    /// Processus du groupe (ou le seul processus lancé), si l'entrée `pid` existe toujours.
    fn pids_of(&self, id: &str, pid: u32) -> Option<Vec<u32>> {
        let p = self.entries.lock().ok()?;
        let e = p.get(id).filter(|e| e.pid == pid)?;
        let mut pids = e.job.as_ref().map(native::job_pids).unwrap_or_default();
        if !pids.contains(&e.pid) {
            pids.push(e.pid);
        }
        Some(pids)
    }

    fn set_ready(&self, id: &str, pid: u32) {
        if let Ok(mut p) = self.entries.lock() {
            if let Some(e) = p.get_mut(id).filter(|e| e.pid == pid) {
                e.ready = true;
            }
        }
    }
}

fn emit(app: &AppHandle) {
    let _ = app.emit(EVENT_UPDATED, app.state::<Running>().view());
}

/// Attend la première fenêtre du logiciel (vérification légère, 4 fois par seconde,
/// 30 s au plus), puis prévient la barre : la tuile quitte l'état « en démarrage ».
fn watch_ready(app: AppHandle, id: String, pid: u32) {
    let spawned = std::thread::Builder::new().name(format!("demarrage-{id}")).spawn(move || {
        let started = Instant::now();
        loop {
            std::thread::sleep(READY_POLL);
            let running = app.state::<Running>();
            let Some(pids) = running.pids_of(&id, pid) else { return };
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
pub fn track(app: &AppHandle, id: &str, mut child: Child) {
    let pid = child.id();
    let handle = child.as_raw_handle() as isize;
    let job = native::job_for(handle);
    if job.is_none() {
        log::warn!("{id} : groupe de processus indisponible, seul le processus lancé sera visé");
    }
    if let Ok(mut p) = app.state::<Running>().entries.lock() {
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
}

/// Demande poliment au logiciel de se fermer (comme sa croix).
pub fn close(app: &AppHandle, id: &str) -> Result<(), String> {
    let running = app.state::<Running>();
    let pids = {
        let mut entries = running.entries.lock().map_err(|_| "État indisponible.")?;
        let e = entries
            .get_mut(id)
            .ok_or("Ce logiciel n'a pas été lancé par le launcher : il ne peut pas le fermer.")?;
        e.close_requested = Some(Instant::now());
        // Le processus lancé et ceux qu'il a lui-même ouverts (même groupe).
        let mut pids = e.job.as_ref().map(native::job_pids).unwrap_or_default();
        if !pids.contains(&e.pid) {
            pids.push(e.pid);
        }
        pids
    };
    let closed = native::close_windows_of(&pids);
    log::info!("{id} : fermeture demandée ({closed} fenêtre(s))");
    Ok(())
}

/// Arrête de force un logiciel lancé par le launcher, qui n'a pas répondu à la fermeture polie.
pub fn force_close(app: &AppHandle, id: &str) -> Result<(), String> {
    let running = app.state::<Running>();
    let entries = running.entries.lock().map_err(|_| "État indisponible.")?;
    let e = entries
        .get(id)
        .ok_or("Ce logiciel n'a pas été lancé par le launcher : il ne peut pas le fermer.")?;
    match e.close_requested {
        Some(t) if t.elapsed() >= FORCE_AFTER => {}
        _ => return Err("Demande d'abord une fermeture normale.".into()),
    }
    // Verrou tenu pendant l'appel : la poignée ne peut pas être libérée entre-temps.
    // Avec un groupe : tout le groupe (le logiciel et les processus qu'il a ouverts).
    let result = match &e.job {
        Some(job) => native::terminate_job(job),
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
        EnumWindows, GetWindow, GetWindowThreadProcessId, IsWindowVisible, PostMessageW, GW_OWNER, WM_CLOSE,
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