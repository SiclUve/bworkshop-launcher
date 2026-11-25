//! Raccourcis clavier globaux.
//!
//! Ils sont seulement **écoutés** : `RegisterHotKey` demande à Windows de prévenir le
//! launcher quand la combinaison est pressée. Rien n'est jamais simulé ni envoyé au jeu.
//! Un fil dédié enregistre les raccourcis et attend leurs messages (attente bloquante :
//! aucun calcul tant qu'on n'appuie sur rien).

use crate::state::AppState;
use crate::widgets::Widgets;
use bworkshop_core::hotkeys::{self, Action, Hotkey, Target};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS, MOD_NOREPEAT};
use windows::Win32::UI::WindowsAndMessaging::{GetMessageW, PeekMessageW, PostThreadMessageW, MSG, PM_NOREMOVE, WM_APP, WM_HOTKEY};

/// État des raccourcis envoyé à la fenêtre principale (page « Raccourcis »).
pub const EVENT_STATUS: &str = "hotkeys-status";
/// Message interne : relire les réglages et réenregistrer.
const WM_RELOAD: u32 = WM_APP + 1;
/// `ERROR_HOTKEY_ALREADY_REGISTERED` sous forme de HRESULT.
const ALREADY_REGISTERED: i32 = 0x8007_0581_u32 as i32;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HotkeyStatus {
    /// « togglePanel », « toggleHideAll », « toggleLock » ou « tool:<id> ».
    pub action: String,
    /// Forme normalisée, si le raccourci est valide.
    pub hotkey: Option<String>,
    /// Vrai : Windows préviendra le launcher.
    pub active: bool,
    pub error: Option<String>,
}

#[derive(Default)]
pub struct Hotkeys {
    thread_id: AtomicU32,
    /// Pendant la saisie d'un nouveau raccourci, plus rien n'est enregistré.
    suspended: AtomicBool,
    status: Mutex<Vec<HotkeyStatus>>,
}

impl Hotkeys {
    pub fn status(&self) -> Vec<HotkeyStatus> {
        self.status.lock().map(|s| s.clone()).unwrap_or_default()
    }
}

/// Démarre le fil des raccourcis.
pub fn start(app: &AppHandle) {
    let app = app.clone();
    let spawned = std::thread::Builder::new().name("raccourcis".into()).spawn(move || {
        let state = app.state::<Hotkeys>();
        state.thread_id.store(unsafe { GetCurrentThreadId() }, Ordering::SeqCst);
        // Crée la file de messages du fil avant le premier PostThreadMessage.
        let mut msg = MSG::default();
        let _ = unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE) };
        let mut registered: Vec<(i32, Target)> = Vec::new();
        apply(&app, &mut registered);
        loop {
            let got = unsafe { GetMessageW(&mut msg, None, 0, 0) };
            if got.0 <= 0 {
                break;
            }
            match msg.message {
                WM_HOTKEY => {
                    let id = msg.wParam.0 as i32;
                    if let Some((_, target)) = registered.iter().find(|(i, _)| *i == id) {
                        run(&app, target);
                    }
                }
                WM_RELOAD => apply(&app, &mut registered),
                _ => {}
            }
        }
    });
    if let Err(e) = spawned {
        log::warn!("Raccourcis indisponibles : {e}");
    }
}

/// Relit les réglages et réenregistre (après une modification, ou pour suspendre).
pub fn reload(app: &AppHandle) {
    let id = app.state::<Hotkeys>().thread_id.load(Ordering::SeqCst);
    if id != 0 {
        let _ = unsafe { PostThreadMessageW(id, WM_RELOAD, WPARAM(0), LPARAM(0)) };
    }
}

/// Suspend les raccourcis pendant qu'on en saisit un nouveau (sinon Windows le garderait pour nous).
pub fn set_suspended(app: &AppHandle, suspended: bool) {
    app.state::<Hotkeys>().suspended.store(suspended, Ordering::SeqCst);
    reload(app);
}

fn apply(app: &AppHandle, registered: &mut Vec<(i32, Target)>) {
    for (id, _) in registered.drain(..) {
        let _ = unsafe { UnregisterHotKey(None, id) };
    }
    let state = app.state::<Hotkeys>();
    let suspended = state.suspended.load(Ordering::SeqCst);
    let bindings = app.state::<AppState>().settings().hotkeys;
    let plan = hotkeys::plan(&bindings);
    let mut status = Vec::new();
    for (target, error) in plan.problems {
        status.push(HotkeyStatus { action: target.key(), hotkey: None, active: false, error: Some(error) });
    }
    for (target, h) in plan.register {
        let mut entry = HotkeyStatus { action: target.key(), hotkey: Some(h.to_string()), active: false, error: None };
        if !suspended {
            let id = registered.len() as i32 + 1;
            match register(id, h) {
                Ok(()) => {
                    registered.push((id, target));
                    entry.active = true;
                }
                Err(e) => {
                    log::warn!("Raccourci {h} non enregistré : {e}");
                    entry.error = Some(e);
                }
            }
        }
        status.push(entry);
    }
    if let Ok(mut s) = state.status.lock() {
        *s = status.clone();
    }
    let _ = app.emit_to(crate::state::MAIN_LABEL, EVENT_STATUS, status);
}

fn register(id: i32, h: Hotkey) -> Result<(), String> {
    let modifiers = HOT_KEY_MODIFIERS(h.modifiers) | MOD_NOREPEAT;
    unsafe { RegisterHotKey(None, id, modifiers, h.vk) }.map_err(|e| {
        if e.code().0 == ALREADY_REGISTERED {
            "déjà utilisé par un autre programme".to_string()
        } else {
            e.message()
        }
    })
}

/// Action d'un raccourci. Afficher / cacher reste instantané : les fenêtres sont gardées.
fn run(app: &AppHandle, target: &Target) {
    let action = match target {
        Target::Action(a) => *a,
        Target::Tool(id) => return toggle_tool(app, id),
        Target::Layout(n) => {
            if let Err(e) = crate::layouts::apply(app, n - 1) {
                log::info!("Raccourci de disposition {n} : {e}");
            }
            return;
        }
    };
    match action {
        Action::TogglePanel => Widgets::toggle_toolbar(app),
        Action::ToggleHideAll => {
            let app2 = app.clone();
            let _ = app.run_on_main_thread(move || crate::widgets::toggle_hide_all(&app2));
        }
        Action::ToggleLock => {
            let app2 = app.clone();
            let _ = app.run_on_main_thread(move || {
                let w = app2.state::<Widgets>();
                let locked = !w.view().locked;
                w.set_locked(&app2, locked);
            });
        }
    }
}

/// Raccourci d'un outil : fait comme sa tuile, selon le mode choisi dans « Mes plugins »
/// (logiciel installé, overlay web ou page web).
fn toggle_tool(app: &AppHandle, id: &str) {
    let (app, id) = (app.clone(), id.to_string());
    // Créer une fenêtre prend un moment : jamais sur le fil des raccourcis ni le fil principal.
    std::thread::spawn(move || {
        if let Err(e) = crate::widgets::toggle_tool(&app, &id) {
            log::info!("Raccourci de {id} : {e}");
            crate::widgets::toolbar_menu::notice(&app, e);
        }
    });
}
