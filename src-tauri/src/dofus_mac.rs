//! Suivi de la fenêtre de Dofus sur Mac : option « Suivre Dofus », **désactivée par défaut**.
//!
//! macOS ne prévient pas une application quand la fenêtre d'une autre bouge. Option activée,
//! un fil relit donc 6 fois par seconde, en lecture seule :
//! - l'application au premier plan (`NSWorkspace.frontmostApplication`, un numéro de processus) ;
//! - la liste des fenêtres à l'écran (`CGWindowListCopyWindowInfo`) : processus, nom de
//!   l'application et position. Dofus est reconnu au nom de son application (« Dofus ») ou,
//!   si l'utilisateur a accordé « Enregistrement de l'écran », au titre de sa fenêtre.
//!
//! Option désactivée, le fil dort (aucune lecture). Rien n'est jamais envoyé à Dofus : seules
//! les fenêtres du launcher bougent ou se cachent.

use crate::mac_sys::{self, WinInfo};
use crate::running::Running;
use crate::state::AppState;
use crate::widgets::{native, Widgets};
use bworkshop_core::dofus_tracker::{is_minimized_rect, title_matches};
use bworkshop_core::geometry::Rect;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tauri::{AppHandle, Manager};

const POLL: Duration = Duration::from_millis(160);

static APP: OnceLock<AppHandle> = OnceLock::new();
static WAKE: OnceLock<mpsc::Sender<()>> = OnceLock::new();
static STATE: Mutex<Tracked> = Mutex::new(Tracked::new());

struct Tracked {
    enabled: bool,
    pattern: String,
    snapping: bool,
    hide_away: bool,
    /// Processus de Dofus reconnu (0 : aucun).
    pid: u32,
    rect: Option<Rect>,
    away: bool,
    hidden: bool,
}

impl Tracked {
    const fn new() -> Self {
        Self { enabled: false, pattern: String::new(), snapping: false, hide_away: false, pid: 0, rect: None, away: false, hidden: false }
    }
}

pub fn start(app: &AppHandle) {
    let _ = APP.set(app.clone());
    let (tx, rx) = mpsc::channel::<()>();
    let _ = WAKE.set(tx);
    let spawned = std::thread::Builder::new().name("suivi-dofus".into()).spawn(move || {
        apply();
        loop {
            let enabled = STATE.lock().map(|s| s.enabled).unwrap_or(false);
            let woke = if enabled { rx.recv_timeout(POLL) } else { rx.recv().map_err(|_| RecvTimeoutError::Disconnected) };
            match woke {
                Ok(()) => apply(),
                Err(RecvTimeoutError::Timeout) => tick(),
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
    });
    if let Err(e) = spawned {
        log::warn!("Suivi de Dofus indisponible : {e}");
    }
}

pub fn reload() {
    if let Some(tx) = WAKE.get() {
        let _ = tx.send(());
    }
}

fn apply() {
    let Some(app) = APP.get() else { return };
    let settings = app.state::<AppState>().settings();
    native::set_snapping(settings.snapping);
    {
        let Ok(mut st) = STATE.lock() else { return };
        st.pattern = settings.dofus_title_pattern.clone();
        st.snapping = settings.snapping;
        st.hide_away = settings.only_when_dofus_foreground;
        st.enabled = settings.follow_dofus;
        if !st.enabled {
            st.pid = 0;
            st.rect = None;
            st.away = false;
        }
    }
    log::info!("Suivi de Dofus : {}", if settings.follow_dofus { "activé" } else { "désactivé" });
    if settings.follow_dofus {
        tick();
    }
    sync_windows();
}

/// Fenêtre du jeu : nom d'application égal au motif (« Dofus »), ou titre reconnu.
fn is_dofus_window(w: &WinInfo, pattern: &str) -> bool {
    let name = if pattern.trim().is_empty() { "Dofus" } else { pattern.trim() };
    w.layer == 0 && (w.owner.eq_ignore_ascii_case(name) || (!w.title.is_empty() && title_matches(&w.title, pattern)))
}

/// Points (macOS) → pixels physiques (Tauri), avec l'échelle de l'écran principal.
fn to_physical(w: &WinInfo, scale: f64) -> Rect {
    let px = |v: f64| (v * scale).round();
    Rect::new(px(w.x) as i32, px(w.y) as i32, px(w.w).max(0.0) as u32, px(w.h).max(0.0) as u32)
}

fn tick() {
    let Some(app) = APP.get() else { return };
    let pattern = match STATE.lock() {
        Ok(st) if st.enabled => st.pattern.clone(),
        _ => return,
    };
    let own_pid = std::process::id();
    let running = app.state::<Running>();
    let front = mac_sys::frontmost_pid();
    let windows = mac_sys::windows_on_screen();
    // Plus grande fenêtre du jeu à l'écran (pas un outil lancé par le launcher).
    let dofus = windows
        .iter()
        .filter(|w| w.pid != own_pid && is_dofus_window(w, &pattern) && !running.owns_pid(w.pid, &pattern))
        .max_by(|a, b| (a.w * a.h).total_cmp(&(b.w * b.h)));
    let scale = app.primary_monitor().ok().flatten().map(|m| m.scale_factor()).unwrap_or(1.0);
    let rect = dofus.map(|w| to_physical(w, scale)).filter(|r| !is_minimized_rect(*r) && r.w > 0 && r.h > 0);
    let own = front == Some(own_pid);
    let launched = front.is_some_and(|p| p != own_pid && running.owns_pid(p, &pattern));
    let front_is_dofus = front.is_some_and(|p| dofus.is_some_and(|d| d.pid == p));
    let moved = {
        let Ok(mut st) = STATE.lock() else { return };
        if !st.enabled {
            return;
        }
        let mut moved = None;
        match (dofus, rect) {
            (Some(d), Some(now)) => {
                if st.pid != d.pid {
                    log::info!("Suivi de Dofus : fenêtre du jeu reconnue");
                    st.pid = d.pid;
                    st.rect = Some(now);
                } else {
                    let before = st.rect.replace(now);
                    moved = before.filter(|b| *b != now).map(|b| (b, now));
                }
            }
            (Some(_), None) => {} // réduit : on garde la dernière position
            (None, _) => {
                st.pid = 0;
                st.rect = None;
            }
        }
        // Jeu au premier plan, launcher ou outil lancé par lui : rien à cacher.
        st.away = front.is_some() && !front_is_dofus && !own && !launched;
        moved
    };
    if let Some((before, after)) = moved {
        let app2 = app.clone();
        let _ = app.run_on_main_thread(move || app2.state::<Widgets>().follow_dofus(before, after));
    }
    sync_windows();
}

/// Position de Dofus, si l'option est activée et le jeu reconnu (pour les dispositions).
pub fn current_rect() -> Option<Rect> {
    STATE.lock().ok().and_then(|st| if st.enabled { st.rect } else { None })
}

fn sync_snap() {
    let target = STATE.lock().ok().and_then(|st| if st.enabled && st.snapping { st.rect } else { None });
    native::set_snap_target(target);
}

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
