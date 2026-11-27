//! Mac : réglages des **fenêtres du launcher uniquement** (jamais de Dofus), même interface
//! que la version Windows (`native_win.rs`).
//!
//! - « HWND » = adresse de la `NSWindow`, gardée en nombre ; chaque fenêtre connue est retenue
//!   ici avec sa `tauri::Window` (déplacement, taille, clics traversants passent par Tauri).
//! - Affichage sans activation : `orderFrontRegardless` (la fenêtre passe devant sans prendre
//!   le clavier ni activer le launcher). Opacité : `alphaValue`.
//! - Les widgets suivent l'utilisateur sur tous les bureaux et par-dessus un jeu en plein écran
//!   (`canJoinAllSpaces` + `fullScreenAuxiliary`), et restent visibles quand le launcher n'est
//!   pas l'application active.
//! - Autorisations de la page (micro, caméra…) : refusées par la vue elle-même
//!   (`on_permission_request`, posé à sa création), avant tout chargement.

use bworkshop_core::geometry::Rect;
use objc2::msg_send;
use objc2::runtime::AnyObject;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::{LogicalSize, PhysicalPosition, PhysicalSize, Webview, Window};

/// Adresse de la `NSWindow` d'une fenêtre du launcher.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HWND(pub isize);

static WINDOWS: Mutex<Option<HashMap<isize, Window>>> = Mutex::new(None);

fn window_of(hwnd: HWND) -> Option<Window> {
    WINDOWS.lock().ok()?.as_ref()?.get(&hwnd.0).cloned()
}

pub fn hwnd(window: &Window) -> Option<HWND> {
    let raw = window.ns_window().ok()? as isize;
    if raw == 0 {
        return None;
    }
    if let Ok(mut w) = WINDOWS.lock() {
        w.get_or_insert_with(HashMap::new).insert(raw, window.clone());
    }
    Some(HWND(raw))
}

fn is_main_thread() -> bool {
    unsafe { libc::pthread_main_np() != 0 }
}

/// Appel AppKit sur la fenêtre, sur le fil principal (tout de suite si on y est déjà).
fn with_ns_window(hwnd: HWND, f: impl FnOnce(&AnyObject) + Send + 'static) {
    let Some(window) = window_of(hwnd) else { return };
    let raw = hwnd.0;
    let call = move || {
        // La fenêtre est toujours connue : elle n'est oubliée qu'à sa destruction, sur ce fil.
        if window_of(HWND(raw)).is_some() {
            f(unsafe { &*(raw as *const AnyObject) });
        }
    };
    if is_main_thread() {
        call();
    } else if let Err(e) = window.run_on_main_thread(call) {
        log::warn!("Réglage de fenêtre impossible : {e}");
    }
}

/// `canJoinAllSpaces` | `ignoresCycle` | `fullScreenAuxiliary`.
const COLLECTION_BEHAVIOR: usize = (1 << 0) | (1 << 6) | (1 << 8);

pub fn apply_style(hwnd: HWND, opacity: f64, click_through: bool) {
    if let Some(window) = window_of(hwnd)
        && let Err(e) = window.set_ignore_cursor_events(click_through)
    {
        log::warn!("Clics traversants non appliqués : {e}");
    }
    let alpha = opacity.clamp(0.0, 1.0);
    with_ns_window(hwnd, move |w| unsafe {
        let _: () = msg_send![w, setAlphaValue: alpha];
        let _: () = msg_send![w, setCollectionBehavior: COLLECTION_BEHAVIOR];
        let _: () = msg_send![w, setHidesOnDeactivate: false];
    });
}

pub fn show_no_activate(hwnd: HWND) {
    with_ns_window(hwnd, |w| unsafe {
        let _: () = msg_send![w, orderFrontRegardless];
    });
}

pub fn hide(hwnd: HWND) {
    with_ns_window(hwnd, |w| unsafe {
        let _: () = msg_send![w, orderOut: std::ptr::null::<AnyObject>()];
    });
}

pub fn is_visible(hwnd: HWND) -> bool {
    window_of(hwnd).and_then(|w| w.is_visible().ok()).unwrap_or(false)
}

/// Déplace et redimensionne (pixels physiques, comme partout dans le launcher).
pub fn set_rect(hwnd: HWND, r: Rect) {
    let Some(window) = window_of(hwnd) else { return };
    let _ = window.set_position(PhysicalPosition::new(r.x, r.y));
    let _ = window.set_size(PhysicalSize::new(r.w, r.h));
}

pub fn window_rect(hwnd: HWND) -> Option<Rect> {
    let window = window_of(hwnd)?;
    let p = window.outer_position().ok()?;
    let s = window.outer_size().ok()?;
    Some(Rect::new(p.x, p.y, s.width, s.height))
}

pub fn scale_of(hwnd: HWND) -> f64 {
    window_of(hwnd).and_then(|w| w.scale_factor().ok()).unwrap_or(1.0)
}

// ---------- Aimantation ----------
//
// Le réglage est retenu ; sur Mac, l'aimantation pendant un déplacement n'est pas encore faite
// (macOS ne laisse pas corriger la position pendant qu'on tire la fenêtre). Le suivi de Dofus,
// lui, fonctionne.

static SNAP_ON: AtomicBool = AtomicBool::new(true);
static SNAP_TARGET: Mutex<Option<Rect>> = Mutex::new(None);

pub fn set_snapping(on: bool) {
    SNAP_ON.store(on, Ordering::SeqCst);
}

pub fn set_snap_target(target: Option<Rect>) {
    if let Ok(mut t) = SNAP_TARGET.lock() {
        *t = target;
    }
}

pub fn install_subclass(_hwnd: HWND) {}

// ---------- Taille minimale (et taille fixe de la pastille) ----------

pub fn set_min_size(hwnd: HWND, w: f64, h: f64) {
    if let Some(window) = window_of(hwnd) {
        let _ = window.set_max_size(None::<LogicalSize<f64>>);
        let _ = window.set_min_size(Some(LogicalSize::new(w, h)));
    }
}

pub fn set_fixed_size(hwnd: HWND, w: f64, h: f64) {
    if let Some(window) = window_of(hwnd) {
        let _ = window.set_min_size(Some(LogicalSize::new(w, h)));
        let _ = window.set_max_size(Some(LogicalSize::new(w, h)));
    }
}

/// Fenêtre détruite : on l'oublie.
pub fn forget_size_rules(hwnd: HWND) {
    if let Ok(mut w) = WINDOWS.lock()
        && let Some(map) = w.as_mut()
    {
        map.remove(&hwnd.0);
    }
}

// ---------- Vue du contenu distant ----------

/// Le refus des autorisations est posé à la création de la vue (`on_permission_request`) :
/// on peut charger la page directement.
pub fn deny_permissions_then_open(webview: &Webview, label: String, url: String) {
    match tauri::Url::parse(&url) {
        Ok(u) => {
            if let Err(e) = webview.navigate(u) {
                log::error!("Widget {label} : chargement de la page impossible ({e})");
            }
        }
        Err(e) => log::error!("Widget {label} : adresse invalide ({e})"),
    }
}

/// Pas de mise en pause d'une page sur Mac : la vue est seulement masquée.
pub fn suspend(_webview: &Webview) {}

pub fn resume(_webview: &Webview) {}
