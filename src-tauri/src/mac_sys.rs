//! Appels système Mac partagés (lecture seule, sauf `terminate_app`) :
//! - liste des fenêtres à l'écran (`CGWindowListCopyWindowInfo`) : processus propriétaire,
//!   position, et titre seulement si l'utilisateur a accordé « Enregistrement de l'écran »
//!   (sans cette autorisation, le titre est simplement absent : rien n'est demandé) ;
//! - application au premier plan (`NSWorkspace.frontmostApplication`) ;
//! - nom et groupe d'un processus (`proc_name`, `proc_listpids`) ;
//! - fermeture polie d'une application lancée par le launcher (`NSRunningApplication.terminate`,
//!   comme ⌘Q).
//!
//! Rien n'est jamais envoyé à Dofus.

use objc2::rc::{autoreleasepool, Retained};
use objc2::runtime::AnyObject;
use objc2::{class, msg_send};
use std::ffi::{c_char, c_void, CStr, CString};

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGWindowListCopyWindowInfo(option: u32, relative_to_window: u32) -> *const c_void;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(cf: *const c_void);
}

const ON_SCREEN_ONLY: u32 = 1 << 0;
const EXCLUDE_DESKTOP_ELEMENTS: u32 = 1 << 4;

/// Une fenêtre à l'écran. Position en points (origine en haut à gauche de l'écran principal).
#[derive(Debug, Clone)]
pub struct WinInfo {
    pub pid: u32,
    pub owner: String,
    /// Vide sans l'autorisation « Enregistrement de l'écran ».
    pub title: String,
    /// 0 : fenêtre normale (les menus, le Dock, etc. ont d'autres niveaux).
    pub layer: i64,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

fn ns_string(s: &str) -> Option<Retained<AnyObject>> {
    let c = CString::new(s).ok()?;
    unsafe { msg_send![class!(NSString), stringWithUTF8String: c.as_ptr()] }
}

unsafe fn to_string(obj: *mut AnyObject) -> String {
    if obj.is_null() {
        return String::new();
    }
    let p: *const c_char = unsafe { msg_send![obj, UTF8String] };
    if p.is_null() { String::new() } else { unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned() }
}

unsafe fn get(dict: *mut AnyObject, key: &str) -> *mut AnyObject {
    match ns_string(key) {
        Some(k) => unsafe { msg_send![dict, objectForKey: &*k] },
        None => std::ptr::null_mut(),
    }
}

unsafe fn number_f64(obj: *mut AnyObject) -> f64 {
    if obj.is_null() { 0.0 } else { unsafe { msg_send![obj, doubleValue] } }
}

/// Fenêtres à l'écran, de la plus en avant à la plus en arrière.
pub fn windows_on_screen() -> Vec<WinInfo> {
    autoreleasepool(|_| unsafe {
        let array = CGWindowListCopyWindowInfo(ON_SCREEN_ONLY | EXCLUDE_DESKTOP_ELEMENTS, 0);
        if array.is_null() {
            return Vec::new();
        }
        let arr = array as *mut AnyObject;
        let n: usize = msg_send![arr, count];
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let d: *mut AnyObject = msg_send![arr, objectAtIndex: i];
            if d.is_null() {
                continue;
            }
            let bounds = get(d, "kCGWindowBounds");
            let (x, y, w, h) = if bounds.is_null() {
                (0.0, 0.0, 0.0, 0.0)
            } else {
                (
                    number_f64(get(bounds, "X")),
                    number_f64(get(bounds, "Y")),
                    number_f64(get(bounds, "Width")),
                    number_f64(get(bounds, "Height")),
                )
            };
            out.push(WinInfo {
                pid: number_f64(get(d, "kCGWindowOwnerPID")) as u32,
                owner: to_string(get(d, "kCGWindowOwnerName")),
                title: to_string(get(d, "kCGWindowName")),
                layer: number_f64(get(d, "kCGWindowLayer")) as i64,
                x,
                y,
                w,
                h,
            });
        }
        CFRelease(array);
        out
    })
}

/// Processus de l'application au premier plan.
pub fn frontmost_pid() -> Option<u32> {
    autoreleasepool(|_| unsafe {
        let ws: Option<Retained<AnyObject>> = msg_send![class!(NSWorkspace), sharedWorkspace];
        let ws = ws?;
        let app: Option<Retained<AnyObject>> = msg_send![&*ws, frontmostApplication];
        let pid: i32 = msg_send![&*app?, processIdentifier];
        (pid > 0).then_some(pid as u32)
    })
}

/// Nom court d'un processus (`Dofus`, `Ankama Launcher`…).
pub fn process_name(pid: u32) -> Option<String> {
    let mut buf = [0u8; 256];
    let n = unsafe { libc::proc_name(pid as i32, buf.as_mut_ptr().cast(), buf.len() as u32) };
    (n > 0).then(|| String::from_utf8_lossy(&buf[..n as usize]).into_owned())
}

/// `PROC_PGRP_ONLY` de `<libproc.h>`.
const PROC_PGRP_ONLY: u32 = 2;

/// Processus encore vivants du groupe `pgid` (le groupe créé au lancement d'un outil).
pub fn group_pids(pgid: u32) -> Vec<u32> {
    let mut buf = vec![0i32; 512];
    let bytes = unsafe {
        libc::proc_listpids(PROC_PGRP_ONLY, pgid, buf.as_mut_ptr().cast(), (buf.len() * size_of::<i32>()) as i32)
    };
    if bytes <= 0 {
        return Vec::new();
    }
    let count = (bytes as usize / size_of::<i32>()).min(buf.len());
    buf[..count].iter().filter(|&&p| p > 0).map(|&p| p as u32).collect()
}

/// Demande poliment à une application de quitter (comme ⌘Q). Faux si ce processus n'est pas
/// une application (pas de fenêtre, outil en ligne de commande).
pub fn terminate_app(pid: u32) -> bool {
    autoreleasepool(|_| unsafe {
        let app: Option<Retained<AnyObject>> =
            msg_send![class!(NSRunningApplication), runningApplicationWithProcessIdentifier: pid as i32];
        match app {
            Some(app) => {
                let ok: bool = msg_send![&*app, terminate];
                ok
            }
            None => false,
        }
    })
}

/// Met un fichier ou un dossier à la corbeille (`NSFileManager.trashItemAtURL`), sans passer
/// par le Finder : aucune autorisation « Automatisation » n'est demandée. Vrai si c'est fait.
pub fn trash(path: &std::path::Path) -> bool {
    let Some(s) = path.to_str() else { return false };
    autoreleasepool(|_| unsafe {
        let Some(p) = ns_string(s) else { return false };
        let url: Option<Retained<AnyObject>> = msg_send![class!(NSURL), fileURLWithPath: &*p];
        let Some(url) = url else { return false };
        let fm: Option<Retained<AnyObject>> = msg_send![class!(NSFileManager), defaultManager];
        let Some(fm) = fm else { return false };
        let ok: bool = msg_send![
            &*fm,
            trashItemAtURL: &*url,
            resultingItemURL: std::ptr::null_mut::<*mut AnyObject>(),
            error: std::ptr::null_mut::<*mut AnyObject>()
        ];
        ok
    })
}
