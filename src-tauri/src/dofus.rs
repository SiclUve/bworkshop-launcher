//! Suivi de la fenêtre de Dofus : option « Suivre Dofus », **désactivée par défaut**.
//!
//! Désactivée, rien n'est demandé à Windows (aucun crochet posé). Activée, le launcher
//! demande seulement à Windows, comme Discord ou OBS :
//! - quelle fenêtre passe au premier plan (`SetWinEventHook(EVENT_SYSTEM_FOREGROUND)`) ;
//! - son titre (`GetWindowTextW`), pour reconnaître Dofus ;
//! - son numéro de processus (`GetWindowThreadProcessId`, un simple nombre, sans poignée),
//!   pour n'écouter que les déplacements de cette fenêtre
//!   (`SetWinEventHook(EVENT_OBJECT_LOCATIONCHANGE)` limité à ce processus) ;
//! - sa position (`DwmGetWindowAttribute(DWMWA_EXTENDED_FRAME_BOUNDS)`).
//!
//! Une fenêtre peut changer de titre juste après être passée devant (le jeu qui démarre,
//! puis affiche le nom du personnage) : si elle n'est pas reconnue, son titre est relu
//! 1 puis 3 secondes plus tard (`GetForegroundWindow` + `GetWindowTextW`, rien de plus).
//!
//! Ce fil applique aussi le réglage « Aimanter » (bords de l'écran et des autres fenêtres du
//! launcher), qui fonctionne sans le suivi.
//!
//! Rien n'est jamais envoyé à Dofus : seules les fenêtres du launcher bougent ou se cachent.
//! Les crochets « hors contexte » ne chargent rien dans le jeu : Windows prévient ce fil.

use crate::running::Running;
use crate::state::AppState;
use crate::widgets::{native, Widgets};
use bworkshop_core::dofus_tracker::{is_minimized_rect, title_matches};
use bworkshop_core::geometry::Rect;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Manager};
use windows::Win32::Foundation::{HWND, LPARAM, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetMessageW, GetWindowTextW, GetWindowThreadProcessId, PeekMessageW, PostThreadMessageW,
    EVENT_OBJECT_LOCATIONCHANGE, EVENT_SYSTEM_FOREGROUND, MSG, OBJID_WINDOW, PM_NOREMOVE, WINEVENT_OUTOFCONTEXT,
    WM_APP,
};

/// Message interne : relire les réglages.
const WM_RELOAD: u32 = WM_APP + 1;
/// Message interne : relire le titre de la fenêtre au premier plan.
const WM_RECHECK: u32 = WM_APP + 2;
/// Relectures du titre après un passage au premier plan non reconnu (délais cumulés : 1 s, 3 s).
const RECHECK_DELAYS_MS: [u64; 2] = [1000, 2000];
/// `CHILDID_SELF` : l'événement concerne la fenêtre elle-même.
const CHILDID_SELF: i32 = 0;

static THREAD_ID: AtomicU32 = AtomicU32::new(0);
/// Numéro de la dernière série de relectures : une nouvelle série annule la précédente.
static RECHECK_GEN: AtomicU32 = AtomicU32::new(0);
static APP: OnceLock<AppHandle> = OnceLock::new();
static STATE: Mutex<Tracked> = Mutex::new(Tracked::new());

/// Ce que le fil de suivi sait. Les poignées sont gardées en nombres (`isize`).
struct Tracked {
    enabled: bool,
    pattern: String,
    snapping: bool,
    hide_away: bool,
    /// Fenêtre de Dofus reconnue, et son numéro de processus.
    dofus: isize,
    pid: u32,
    /// Dernière position connue (pas réduite).
    rect: Option<Rect>,
    /// Une autre fenêtre (ni Dofus ni le launcher) est au premier plan.
    away: bool,
    /// Fenêtres du launcher cachées pour cette raison.
    hidden: bool,
    foreground_hook: isize,
    location_hook: isize,
}

impl Tracked {
    const fn new() -> Self {
        Self {
            enabled: false,
            pattern: String::new(),
            snapping: false,
            hide_away: false,
            dofus: 0,
            pid: 0,
            rect: None,
            away: false,
            hidden: false,
            foreground_hook: 0,
            location_hook: 0,
        }
    }
}

/// Démarre le fil de suivi. Il ne pose ses crochets que si l'option est activée.
pub fn start(app: &AppHandle) {
    let _ = APP.set(app.clone());
    let spawned = std::thread::Builder::new().name("suivi-dofus".into()).spawn(|| {
        THREAD_ID.store(unsafe { GetCurrentThreadId() }, Ordering::SeqCst);
        let mut msg = MSG::default();
        // Crée la file de messages du fil avant le premier PostThreadMessage.
        let _ = unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE) };
        apply();
        // Les crochets hors contexte sont appelés pendant GetMessageW, sur ce fil.
        loop {
            let got = unsafe { GetMessageW(&mut msg, None, 0, 0) };
            if got.0 <= 0 {
                break;
            }
            if msg.message == WM_RELOAD {
                apply();
            } else if msg.message == WM_RECHECK && msg.wParam.0 as u32 == RECHECK_GEN.load(Ordering::SeqCst) {
                on_foreground_checked(unsafe { GetForegroundWindow() }, false);
            }
        }
    });
    if let Err(e) = spawned {
        log::warn!("Suivi de Dofus indisponible : {e}");
    }
}

/// Relit les réglages (après une modification).
pub fn reload() {
    let id = THREAD_ID.load(Ordering::SeqCst);
    if id != 0 {
        let _ = unsafe { PostThreadMessageW(id, WM_RELOAD, WPARAM(0), LPARAM(0)) };
    }
}

fn unhook(raw: &mut isize) {
    if *raw != 0 {
        let _ = unsafe { UnhookWinEvent(HWINEVENTHOOK(*raw as _)) };
        *raw = 0;
    }
}

/// Applique les réglages : pose ou retire les crochets.
fn apply() {
    let Some(app) = APP.get() else { return };
    let settings = app.state::<AppState>().settings();
    // L'aimantation (écran, autres fenêtres) ne dépend pas du suivi de Dofus.
    native::set_snapping(settings.snapping);
    let first_time;
    {
        let Ok(mut st) = STATE.lock() else { return };
        st.pattern = settings.dofus_title_pattern.clone();
        st.snapping = settings.snapping;
        st.hide_away = settings.only_when_dofus_foreground;
        first_time = settings.follow_dofus && !st.enabled;
        st.enabled = settings.follow_dofus;
        if !st.enabled {
            unhook(&mut st.foreground_hook);
            unhook(&mut st.location_hook);
            st.dofus = 0;
            st.pid = 0;
            st.rect = None;
            st.away = false;
        } else if st.foreground_hook == 0 {
            let hook = unsafe {
                SetWinEventHook(
                    EVENT_SYSTEM_FOREGROUND,
                    EVENT_SYSTEM_FOREGROUND,
                    None,
                    Some(on_event),
                    0,
                    0,
                    WINEVENT_OUTOFCONTEXT,
                )
            };
            if hook.is_invalid() {
                log::warn!("Suivi de Dofus : Windows a refusé l'écoute du premier plan");
            } else {
                st.foreground_hook = hook.0 as isize;
            }
        }
    }
    log::info!("Suivi de Dofus : {}", if settings.follow_dofus { "activé" } else { "désactivé" });
    if first_time {
        // La fenêtre déjà au premier plan, sans attendre le prochain changement.
        on_foreground(unsafe { GetForegroundWindow() });
    }
    sync_windows();
}

/// Position visible d'une fenêtre (sans l'ombre), en pixels physiques.
fn frame_rect(hwnd: HWND) -> Option<Rect> {
    let mut r = RECT::default();
    unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut r as *mut RECT as *mut _,
            std::mem::size_of::<RECT>() as u32,
        )
    }
    .ok()?;
    Some(Rect::new(r.left, r.top, (r.right - r.left).max(0) as u32, (r.bottom - r.top).max(0) as u32))
}

fn title_of(hwnd: HWND) -> String {
    let mut buf = [0u16; 512];
    let n = unsafe { GetWindowTextW(hwnd, &mut buf) }.max(0) as usize;
    String::from_utf16_lossy(&buf[..n.min(buf.len())])
}

unsafe extern "system" fn on_event(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    id_object: i32,
    id_child: i32,
    _thread: u32,
    _time: u32,
) {
    if event == EVENT_SYSTEM_FOREGROUND {
        on_foreground(hwnd);
    } else if event == EVENT_OBJECT_LOCATIONCHANGE && id_object == OBJID_WINDOW.0 && id_child == CHILDID_SELF {
        on_location(hwnd);
    }
}

/// Relit le titre de la fenêtre au premier plan un peu plus tard (série `generation`).
fn schedule_rechecks(generation: u32) {
    let _ = std::thread::Builder::new().name("suivi-dofus-relecture".into()).spawn(move || {
        for ms in RECHECK_DELAYS_MS {
            std::thread::sleep(std::time::Duration::from_millis(ms));
            if RECHECK_GEN.load(Ordering::SeqCst) != generation {
                return; // une autre fenêtre est passée devant entre-temps
            }
            let id = THREAD_ID.load(Ordering::SeqCst);
            if id != 0 {
                let _ = unsafe { PostThreadMessageW(id, WM_RECHECK, WPARAM(generation as usize), LPARAM(0)) };
            }
        }
    });
}

/// Une fenêtre passe au premier plan : est-ce Dofus, le launcher, ou autre chose ?
fn on_foreground(hwnd: HWND) {
    on_foreground_checked(hwnd, true);
}

/// `fresh` : vrai pour un vrai passage au premier plan (faux pour une relecture du titre).
fn on_foreground_checked(hwnd: HWND, fresh: bool) {
    let Some(app) = APP.get() else { return };
    if hwnd.0.is_null() {
        return;
    }
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    let own = pid == std::process::id();
    // Un outil démarré par le launcher peut avoir « Dofus » dans son titre : ce n'est pas le jeu.
    let launched = !own && app.state::<Running>().owns_pid(pid);
    let pattern = match STATE.lock() {
        Ok(st) if st.enabled => st.pattern.clone(),
        _ => return,
    };
    let is_dofus = !own && !launched && title_matches(&title_of(hwnd), &pattern);
    if fresh {
        // Toute nouvelle fenêtre au premier plan annule les relectures en attente.
        let generation = RECHECK_GEN.fetch_add(1, Ordering::SeqCst).wrapping_add(1);
        if !is_dofus && !own && !launched {
            schedule_rechecks(generation);
        }
    } else if !is_dofus {
        return; // relecture sans changement : rien à faire
    }
    let rect = if is_dofus { frame_rect(hwnd) } else { None };
    {
        let Ok(mut st) = STATE.lock() else { return };
        if !st.enabled {
            return;
        }
        if is_dofus {
            if st.pid != pid || st.location_hook == 0 {
                unhook(&mut st.location_hook);
                let hook = unsafe {
                    SetWinEventHook(
                        EVENT_OBJECT_LOCATIONCHANGE,
                        EVENT_OBJECT_LOCATIONCHANGE,
                        None,
                        Some(on_event),
                        pid,
                        0,
                        WINEVENT_OUTOFCONTEXT,
                    )
                };
                if !hook.is_invalid() {
                    st.location_hook = hook.0 as isize;
                }
                log::info!("Suivi de Dofus : fenêtre du jeu reconnue");
            }
            st.dofus = hwnd.0 as isize;
            st.pid = pid;
            if let Some(r) = rect.filter(|r| !is_minimized_rect(*r)) {
                st.rect = Some(r);
            }
            st.away = false;
        } else {
            // Dofus fermé entre-temps : on l'oublie (plus d'aimantation vers une fenêtre disparue).
            if st.dofus != 0 && frame_rect(HWND(st.dofus as _)).is_none() {
                unhook(&mut st.location_hook);
                st.dofus = 0;
                st.pid = 0;
                st.rect = None;
            }
            // Un outil lancé par le launcher accompagne le jeu : ses fenêtres ne cachent rien.
            st.away = !own && !launched;
        }
    }
    sync_windows();
}

/// La fenêtre de Dofus a bougé : les fenêtres du launcher posées dessus suivent.
fn on_location(hwnd: HWND) {
    let Some(app) = APP.get() else { return };
    let moved = {
        let Ok(mut st) = STATE.lock() else { return };
        if !st.enabled || st.dofus != hwnd.0 as isize {
            return;
        }
        let Some(now) = frame_rect(hwnd) else { return };
        if is_minimized_rect(now) {
            return; // réduit : on garde la dernière position, pour la retrouver au retour
        }
        let before = st.rect.replace(now);
        before.filter(|b| *b != now).map(|b| (b, now))
    };
    if let Some((before, after)) = moved {
        let app2 = app.clone();
        let _ = app.run_on_main_thread(move || app2.state::<Widgets>().follow_dofus(before, after));
    }
    sync_snap();
}

/// Position de Dofus, si l'option est activée et le jeu reconnu (pour les dispositions).
pub fn current_rect() -> Option<Rect> {
    STATE.lock().ok().and_then(|st| if st.enabled { st.rect } else { None })
}

fn sync_snap() {
    let target = STATE.lock().ok().and_then(|st| if st.enabled && st.snapping { st.rect } else { None });
    native::set_snap_target(target);
}

/// Aligne l'aimantation et l'affichage des fenêtres du launcher sur l'état du suivi.
fn sync_windows() {
    sync_snap();
    let Some(app) = APP.get() else { return };
    let change = {
        let Ok(mut st) = STATE.lock() else { return };
        let want = st.enabled && st.hide_away && st.away;
        if want == st.hidden {
            None
        } else {
            st.hidden = want;
            Some(want)
        }
    };
    if let Some(away) = change {
        let app2 = app.clone();
        let _ = app.run_on_main_thread(move || app2.state::<Widgets>().set_away(away));
    }
}
