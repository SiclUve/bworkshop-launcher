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
#[cfg(windows)]
use windows::Win32::Foundation::{LPARAM, WPARAM};
#[cfg(windows)]
use windows::Win32::System::Threading::GetCurrentThreadId;
#[cfg(windows)]
use windows::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS, MOD_NOREPEAT};
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::{GetMessageW, PeekMessageW, PostThreadMessageW, MSG, PM_NOREMOVE, WM_APP, WM_HOTKEY};

/// État des raccourcis envoyé à la fenêtre principale (page « Raccourcis »).
pub const EVENT_STATUS: &str = "hotkeys-status";
/// Message interne : relire les réglages et réenregistrer.
#[cfg(windows)]
const WM_RELOAD: u32 = WM_APP + 1;
/// `ERROR_HOTKEY_ALREADY_REGISTERED` sous forme de HRESULT.
#[cfg(windows)]
const ALREADY_REGISTERED: i32 = 0x8007_0581_u32 as i32;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HotkeyStatus {
    /// « togglePanel », « toggleHideAll », « toggleLock », « layout:<n> » ou « tool:<id> ».
    pub action: String,
    /// Forme normalisée, si le raccourci est valide.
    pub hotkey: Option<String>,
    /// Vrai : Windows préviendra le launcher.
    pub active: bool,
    pub error: Option<String>,
}

#[derive(Default)]
pub struct Hotkeys {
    #[cfg_attr(not(windows), allow(dead_code))]
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
#[cfg(windows)]
pub fn start(app: &AppHandle) {
    let app = app.clone();
    let spawned = std::thread::Builder::new().name("raccourcis".into()).spawn(move || {
        let state = app.state::<Hotkeys>();
        state.thread_id.store(unsafe { GetCurrentThreadId() }, Ordering::SeqCst);
        // Crée la file de messages du fil avant le premier PostThreadMessage.
        let mut msg = MSG::default();
        let _ = unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE) };
        let mut registered: Vec<(i32, Target)> = Vec::new();
        let mut next = 0;
        apply(&app, &mut registered, |h| {
            next += 1;
            register(next, h).map(|_| next)
        });
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
                WM_RELOAD => {
                    for (id, _) in registered.drain(..) {
                        let _ = unsafe { UnregisterHotKey(None, id) };
                    }
                    let mut next = 0;
                    apply(&app, &mut registered, |h| {
                        next += 1;
                        register(next, h).map(|_| next)
                    });
                }
                _ => {}
            }
        }
    });
    if let Err(e) = spawned {
        log::warn!("Raccourcis indisponibles : {e}");
    }
}

/// Relit les réglages et réenregistre (après une modification, ou pour suspendre).
#[cfg(windows)]
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

/// Lève la suspension si elle est encore active (fenêtre principale détruite pendant une saisie).
pub fn resume_if_suspended(app: &AppHandle) {
    if app.state::<Hotkeys>().suspended.swap(false, Ordering::SeqCst) {
        log::info!("Raccourcis réactivés : la fenêtre a été fermée pendant une saisie");
        reload(app);
    }
}

/// Calcule les raccourcis à enregistrer, les enregistre (`register` rend leur numéro) et
/// envoie leur état à la fenêtre principale. Les anciens ont déjà été retirés.
fn apply<K>(app: &AppHandle, registered: &mut Vec<(K, Target)>, mut register: impl FnMut(Hotkey) -> Result<K, String>) {
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
            match register(h) {
                Ok(id) => {
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

#[cfg(windows)]
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

// ---------- Mac ----------
//
// Raccourcis enregistrés auprès de macOS (`RegisterEventHotKey`, API Carbon) : comme sous
// Windows, le launcher est seulement prévenu ; aucune autorisation « Surveillance de l'entrée »
// n'est demandée. Le gestionnaire vit sur le fil principal (exigence de macOS).

#[cfg(target_os = "macos")]
mod mac {
    use super::*;
    use global_hotkey::hotkey::{Code, HotKey, Modifiers};
    use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
    use std::cell::RefCell;

    thread_local! {
        /// Gestionnaire et raccourcis enregistrés (fil principal seulement).
        static MANAGER: RefCell<Option<(GlobalHotKeyManager, Vec<HotKey>)>> = const { RefCell::new(None) };
    }
    /// Numéro de raccourci → action (lu par le gestionnaire d'événements).
    static TARGETS: Mutex<Vec<(u32, Target)>> = Mutex::new(Vec::new());

    /// Touche virtuelle Windows (format des réglages) → touche Mac.
    pub(super) fn code_of(vk: u32) -> Option<Code> {
        const LETTERS: [Code; 26] = [
            Code::KeyA, Code::KeyB, Code::KeyC, Code::KeyD, Code::KeyE, Code::KeyF, Code::KeyG, Code::KeyH, Code::KeyI,
            Code::KeyJ, Code::KeyK, Code::KeyL, Code::KeyM, Code::KeyN, Code::KeyO, Code::KeyP, Code::KeyQ, Code::KeyR,
            Code::KeyS, Code::KeyT, Code::KeyU, Code::KeyV, Code::KeyW, Code::KeyX, Code::KeyY, Code::KeyZ,
        ];
        const DIGITS: [Code; 10] = [
            Code::Digit0, Code::Digit1, Code::Digit2, Code::Digit3, Code::Digit4, Code::Digit5, Code::Digit6, Code::Digit7,
            Code::Digit8, Code::Digit9,
        ];
        const NUMPAD: [Code; 10] = [
            Code::Numpad0, Code::Numpad1, Code::Numpad2, Code::Numpad3, Code::Numpad4, Code::Numpad5, Code::Numpad6,
            Code::Numpad7, Code::Numpad8, Code::Numpad9,
        ];
        const FKEYS: [Code; 24] = [
            Code::F1, Code::F2, Code::F3, Code::F4, Code::F5, Code::F6, Code::F7, Code::F8, Code::F9, Code::F10, Code::F11,
            Code::F12, Code::F13, Code::F14, Code::F15, Code::F16, Code::F17, Code::F18, Code::F19, Code::F20, Code::F21,
            Code::F22, Code::F23, Code::F24,
        ];
        Some(match vk {
            0x41..=0x5A => LETTERS[(vk - 0x41) as usize],
            0x30..=0x39 => DIGITS[(vk - 0x30) as usize],
            0x60..=0x69 => NUMPAD[(vk - 0x60) as usize],
            0x70..=0x87 => FKEYS[(vk - 0x70) as usize],
            0x20 => Code::Space,
            0x21 => Code::PageUp,
            0x22 => Code::PageDown,
            0x23 => Code::End,
            0x24 => Code::Home,
            0x25 => Code::ArrowLeft,
            0x26 => Code::ArrowUp,
            0x27 => Code::ArrowRight,
            0x28 => Code::ArrowDown,
            0x2D => Code::Insert,
            0x2E => Code::Delete,
            _ => return None,
        })
    }

    /// Ctrl → Contrôle, Alt → Option, Maj → Maj, Win → Commande (⌘).
    pub(super) fn modifiers_of(m: u32) -> Modifiers {
        use bworkshop_core::hotkeys::{MOD_ALT, MOD_CONTROL, MOD_SHIFT, MOD_WIN};
        let mut out = Modifiers::empty();
        if m & MOD_ALT != 0 {
            out |= Modifiers::ALT;
        }
        if m & MOD_CONTROL != 0 {
            out |= Modifiers::CONTROL;
        }
        if m & MOD_SHIFT != 0 {
            out |= Modifiers::SHIFT;
        }
        if m & MOD_WIN != 0 {
            out |= Modifiers::SUPER;
        }
        out
    }

    pub fn start(app: &AppHandle) {
        let handler_app = app.clone();
        GlobalHotKeyEvent::set_event_handler(Some(move |event: GlobalHotKeyEvent| {
            if event.state != HotKeyState::Pressed {
                return;
            }
            let target =
                TARGETS.lock().ok().and_then(|t| t.iter().find(|(id, _)| *id == event.id).map(|(_, t)| t.clone()));
            if let Some(target) = target {
                run(&handler_app, &target);
            }
        }));
        reload(app);
    }

    pub fn reload(app: &AppHandle) {
        let app2 = app.clone();
        let _ = app.run_on_main_thread(move || apply_on_main(&app2));
    }

    fn apply_on_main(app: &AppHandle) {
        MANAGER.with(|cell| {
            let mut slot = cell.borrow_mut();
            if slot.is_none() {
                match GlobalHotKeyManager::new() {
                    Ok(m) => *slot = Some((m, Vec::new())),
                    Err(e) => log::warn!("Raccourcis indisponibles : {e}"),
                }
            }
            let Some((manager, keys)) = slot.as_mut() else { return };
            for k in keys.drain(..) {
                let _ = manager.unregister(k);
            }
            let mut registered: Vec<(u32, Target)> = Vec::new();
            apply(app, &mut registered, |h| {
                let code = code_of(h.vk).ok_or_else(|| "touche indisponible sur Mac".to_string())?;
                let key = HotKey::new(Some(modifiers_of(h.modifiers)), code);
                manager.register(key).map_err(|e| match e {
                    global_hotkey::Error::AlreadyRegistered(_) => "déjà utilisé par un autre programme".to_string(),
                    other => other.to_string(),
                })?;
                keys.push(key);
                Ok(key.id())
            });
            if let Ok(mut t) = TARGETS.lock() {
                *t = registered;
            }
        });
    }
}

#[cfg(target_os = "macos")]
pub use mac::{reload, start};
