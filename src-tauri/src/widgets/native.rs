//! Appels Win32 sur **les fenêtres du launcher uniquement** (jamais sur Dofus).
//!
//! Pourquoi ne pas passer par Tauri pour afficher, déplacer ou rendre une fenêtre
//! traversante : chaque changement d'état fait par Tao rappelle `ShowWindow(SW_SHOW)`,
//! qui active la fenêtre et volerait le focus au jeu. Les widgets sont donc
//! affichés sans activation (`SW_SHOWNOACTIVATE`) et leur style est posé ici.

use bworkshop_core::geometry::Rect;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use tauri::{Webview, Window};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowLongW, GetWindowRect, IsWindowVisible, SetLayeredWindowAttributes, SetWindowLongW, SetWindowPos,
    ShowWindow, GWL_EXSTYLE, LWA_ALPHA, MINMAXINFO, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    SWP_NOZORDER, SW_HIDE, SW_SHOWNOACTIVATE, WM_GETMINMAXINFO, WM_MOVING, WS_EX_LAYERED, WS_EX_TOOLWINDOW,
    WS_EX_TRANSPARENT,
};

pub fn hwnd(window: &Window) -> Option<HWND> {
    window.hwnd().ok().map(|h| HWND(h.0 as _))
}

/// Pose le style étendu d'un widget :
/// - `WS_EX_TOOLWINDOW` : ni barre des tâches, ni Alt+Tab ;
/// - `WS_EX_LAYERED` + alpha : opacité de toute la fenêtre ;
/// - `WS_EX_TRANSPARENT` si verrouillé : les clics passent à travers.
pub fn apply_style(hwnd: HWND, opacity: f64, click_through: bool) {
    unsafe {
        let mut ex = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
        ex |= WS_EX_LAYERED.0 | WS_EX_TOOLWINDOW.0;
        if click_through {
            ex |= WS_EX_TRANSPARENT.0;
        } else {
            ex &= !WS_EX_TRANSPARENT.0;
        }
        SetWindowLongW(hwnd, GWL_EXSTYLE, ex as i32);
        let alpha = (opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
        if let Err(e) = SetLayeredWindowAttributes(hwnd, Default::default(), alpha, LWA_ALPHA) {
            log::warn!("Opacité non appliquée : {e}");
        }
        let _ = SetWindowPos(
            hwnd,
            None,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
    }
}

pub fn show_no_activate(hwnd: HWND) {
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    }
}

pub fn hide(hwnd: HWND) {
    unsafe {
        let _ = ShowWindow(hwnd, SW_HIDE);
    }
}

pub fn is_visible(hwnd: HWND) -> bool {
    unsafe { IsWindowVisible(hwnd).as_bool() }
}

/// Déplace et redimensionne sans activer ni changer l'ordre d'affichage.
pub fn set_rect(hwnd: HWND, r: Rect) {
    unsafe {
        let _ = SetWindowPos(hwnd, None, r.x, r.y, r.w as i32, r.h as i32, SWP_NOZORDER | SWP_NOACTIVATE);
    }
}

pub fn window_rect(hwnd: HWND) -> Option<Rect> {
    let mut r = RECT::default();
    unsafe { GetWindowRect(hwnd, &mut r) }.ok()?;
    Some(Rect::new(r.left, r.top, (r.right - r.left).max(0) as u32, (r.bottom - r.top).max(0) as u32))
}

// ---------- Taille minimale (et taille fixe de la pastille) ----------

#[derive(Clone, Copy)]
enum SizeRule {
    /// Taille minimale, en pixels logiques.
    Min(f64, f64),
    /// Taille imposée (pastille), en pixels logiques.
    Fixed(f64, f64),
}

fn rules() -> &'static Mutex<HashMap<isize, SizeRule>> {
    static RULES: OnceLock<Mutex<HashMap<isize, SizeRule>>> = OnceLock::new();
    RULES.get_or_init(Default::default)
}

const SUBCLASS_ID: usize = 0xB0B5;

/// Échelle de l'écran d'une fenêtre du launcher (1.0 à 100 %).
pub fn scale_of(hwnd: HWND) -> f64 {
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    if dpi > 0 { dpi as f64 / 96.0 } else { 1.0 }
}

// ---------- Aimantation (réglage « Aimanter », activé par défaut) ----------

/// Aimantation active (réglage). Les cibles : bords de l'écran, autres fenêtres du launcher,
/// et la fenêtre de Dofus si l'option « Suivre Dofus » est activée.
static SNAP_ON: AtomicBool = AtomicBool::new(true);
/// Rectangle de Dofus, posé par le suivi (`None` : suivi désactivé ou jeu inconnu).
static SNAP_TARGET: Mutex<Option<Rect>> = Mutex::new(None);
/// Fenêtres du launcher sous-classées (widgets, barre) : cibles les unes des autres.
static SNAP_WINDOWS: Mutex<Vec<isize>> = Mutex::new(Vec::new());

pub fn set_snapping(on: bool) {
    SNAP_ON.store(on, Ordering::SeqCst);
}

pub fn set_snap_target(target: Option<Rect>) {
    if let Ok(mut t) = SNAP_TARGET.lock() {
        *t = target;
    }
}

/// Zone de travail (écran sans la barre des tâches) de l'écran de cette fenêtre du launcher.
fn work_area(hwnd: HWND) -> Option<Rect> {
    use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST};
    let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
    unsafe { GetMonitorInfoW(monitor, &mut info) }.ok().ok()?;
    let w = info.rcWork;
    Some(Rect::new(w.left, w.top, (w.right - w.left).max(0) as u32, (w.bottom - w.top).max(0) as u32))
}

/// Pendant qu'on déplace **une fenêtre du launcher** (`WM_MOVING`), colle ses bords à moins
/// de 12 px des bords de l'écran, des autres fenêtres du launcher et de Dofus (si suivi).
/// Maj enfoncée : pas d'aimantation. L'état de Maj n'est lu qu'ici, pendant ce déplacement.
fn snap_moving(hwnd: HWND, lparam: LPARAM) -> bool {
    if !SNAP_ON.load(Ordering::SeqCst) {
        return false;
    }
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_SHIFT};
    if unsafe { GetAsyncKeyState(VK_SHIFT.0 as i32) } as u16 & 0x8000 != 0 {
        return false;
    }
    let mut targets: Vec<Rect> = Vec::new();
    if let Some(dofus) = SNAP_TARGET.lock().ok().and_then(|t| *t) {
        targets.push(dofus);
    }
    targets.extend(work_area(hwnd));
    let others: Vec<isize> = SNAP_WINDOWS.lock().map(|w| w.clone()).unwrap_or_default();
    for raw in others.into_iter().filter(|r| *r != hwnd.0 as isize) {
        let other = HWND(raw as _);
        if is_visible(other) {
            targets.extend(window_rect(other));
        }
    }
    let r = unsafe { &mut *(lparam.0 as *mut RECT) };
    let moving = Rect::new(r.left, r.top, (r.right - r.left).max(0) as u32, (r.bottom - r.top).max(0) as u32);
    let distance = (bworkshop_core::dofus_tracker::SNAP_DISTANCE * scale_of(hwnd)).round() as i32;
    let snapped = bworkshop_core::dofus_tracker::snap_many(moving, &targets, distance);
    if snapped == moving {
        return false;
    }
    *r = RECT { left: snapped.x, top: snapped.y, right: snapped.x + snapped.w as i32, bottom: snapped.y + snapped.h as i32 };
    true
}

unsafe extern "system" fn subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    let result = unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) };
    if msg == WM_MOVING && lparam.0 != 0 && snap_moving(hwnd, lparam) {
        return LRESULT(1);
    }
    if msg == WM_GETMINMAXINFO && lparam.0 != 0 {
        let rule = rules().lock().ok().and_then(|r| r.get(&(hwnd.0 as isize)).copied());
        if let Some(rule) = rule {
            let scale = scale_of(hwnd);
            let px = |v: f64| (v * scale).round() as i32;
            let info = unsafe { &mut *(lparam.0 as *mut MINMAXINFO) };
            match rule {
                SizeRule::Min(w, h) => {
                    info.ptMinTrackSize.x = px(w);
                    info.ptMinTrackSize.y = px(h);
                }
                SizeRule::Fixed(w, h) => {
                    info.ptMinTrackSize.x = px(w);
                    info.ptMinTrackSize.y = px(h);
                    info.ptMaxTrackSize.x = px(w);
                    info.ptMaxTrackSize.y = px(h);
                }
            }
        }
    }
    result
}

/// Sous-classe **notre propre fenêtre** (widget ou barre) : taille minimale (`WM_GETMINMAXINFO`)
/// et aimantation pendant un déplacement (`WM_MOVING`).
pub fn install_subclass(hwnd: HWND) {
    if let Ok(mut w) = SNAP_WINDOWS.lock() {
        if !w.contains(&(hwnd.0 as isize)) {
            w.push(hwnd.0 as isize);
        }
    }
    unsafe {
        if !SetWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID, 0).as_bool() {
            log::warn!("Taille minimale des widgets non appliquée (sous-classement refusé)");
        }
    }
}

pub fn set_min_size(hwnd: HWND, w: f64, h: f64) {
    if let Ok(mut r) = rules().lock() {
        r.insert(hwnd.0 as isize, SizeRule::Min(w, h));
    }
}

pub fn set_fixed_size(hwnd: HWND, w: f64, h: f64) {
    if let Ok(mut r) = rules().lock() {
        r.insert(hwnd.0 as isize, SizeRule::Fixed(w, h));
    }
}

pub fn forget_size_rules(hwnd: HWND) {
    if let Ok(mut w) = SNAP_WINDOWS.lock() {
        w.retain(|x| *x != hwnd.0 as isize);
    }
    if let Ok(mut r) = rules().lock() {
        r.remove(&(hwnd.0 as isize));
    }
}

// ---------- WebView2 du contenu distant ----------

/// Refuse toute demande d'autorisation de la page (micro, caméra, position,
/// notifications, presse-papiers…).
pub fn deny_permissions(webview: &Webview, label: String) {
    let result = webview.with_webview(move |pw| unsafe {
        use webview2_com::Microsoft::Web::WebView2::Win32::{
            ICoreWebView2PermissionRequestedEventArgs, COREWEBVIEW2_PERMISSION_KIND, COREWEBVIEW2_PERMISSION_STATE_DENY,
        };
        use webview2_com::PermissionRequestedEventHandler;
        let core = match pw.controller().CoreWebView2() {
            Ok(c) => c,
            Err(e) => {
                log::error!("Widget {label} : WebView2 introuvable pour bloquer les autorisations ({e})");
                return;
            }
        };
        let label2 = label.clone();
        let handler = PermissionRequestedEventHandler::create(Box::new(
            move |_sender, args: Option<ICoreWebView2PermissionRequestedEventArgs>| {
                if let Some(args) = args {
                    let mut kind = COREWEBVIEW2_PERMISSION_KIND::default();
                    let _ = args.PermissionKind(&mut kind);
                    args.SetState(COREWEBVIEW2_PERMISSION_STATE_DENY)?;
                    log::info!("Widget {label2} : autorisation refusée (type {})", kind.0);
                }
                Ok(())
            },
        ));
        let mut token = 0i64;
        if let Err(e) = core.add_PermissionRequested(&handler, &mut token) {
            log::error!("Widget {label} : impossible de bloquer les autorisations ({e})");
        }
    });
    if let Err(e) = result {
        log::error!("Accès au WebView2 impossible : {e}");
    }
}

/// Met en pause la page (pastille). La vue doit déjà être masquée.
pub fn suspend(webview: &Webview) {
    let _ = webview.with_webview(|pw| unsafe {
        use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2_3;
        use webview2_com::TrySuspendCompletedHandler;
        use windows::core::Interface;
        let Ok(core) = pw.controller().CoreWebView2() else { return };
        let Ok(core3) = core.cast::<ICoreWebView2_3>() else {
            log::warn!("Mise en pause indisponible (WebView2 trop ancien)");
            return;
        };
        let handler = TrySuspendCompletedHandler::create(Box::new(|result, ok| {
            match (result, ok) {
                (Ok(()), true) => log::info!("Widget en pause"),
                (r, _) => log::warn!("Mise en pause refusée par WebView2 ({r:?})"),
            }
            Ok(())
        }));
        if let Err(e) = core3.TrySuspend(&handler) {
            log::warn!("Mise en pause impossible : {e}");
        }
    });
}

pub fn resume(webview: &Webview) {
    let _ = webview.with_webview(|pw| unsafe {
        use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2_3;
        use windows::core::Interface;
        if let Ok(core3) = pw.controller().CoreWebView2().and_then(|c| c.cast::<ICoreWebView2_3>()) {
            let _ = core3.Resume();
        }
    });
}
