//! Option « Suivre Dofus » côté fenêtres du launcher : suivre le jeu quand il bouge,
//! et se cacher quand il n'est plus au premier plan. Seules les fenêtres du launcher
//! (widgets et barre) sont déplacées ou cachées, jamais celle de Dofus.

use super::{native, Widgets};
use bworkshop_core::dofus_tracker::{self, SNAP_DISTANCE};
use bworkshop_core::geometry::Rect;
use std::sync::atomic::Ordering;
use super::native::HWND;

impl Widgets {
    /// Widgets ouverts et barre d'outils.
    fn launcher_hwnds(&self) -> Vec<HWND> {
        self.live_hwnds().into_iter().map(|(_, h)| h).chain(self.toolbar_hwnd_for_all()).collect()
    }

    /// Dofus est passé de `before` à `after` : les fenêtres posées sur lui (ou collées
    /// à ses bords) bougent d'autant. À appeler sur le fil principal.
    pub fn follow_dofus(&self, before: Rect, after: Rect) {
        for hwnd in self.launcher_hwnds() {
            let Some(r) = native::window_rect(hwnd) else { continue };
            let distance = (SNAP_DISTANCE * native::scale_of(hwnd)).round() as i32;
            if let Some(next) = dofus_tracker::follow(r, before, after, distance) {
                native::set_rect(hwnd, next);
            }
        }
    }

    /// Dofus n'est plus au premier plan (`away`) : cache les fenêtres visibles et s'en souvient.
    /// Il revient : réaffiche exactement celles-là (sans réveiller ce que l'utilisateur a caché).
    /// À appeler sur le fil principal.
    pub fn set_away(&self, away: bool) {
        let Ok(mut hidden) = self.away.lock() else { return };
        if away {
            if self.hidden_all.load(Ordering::SeqCst) {
                return;
            }
            for hwnd in self.launcher_hwnds() {
                if native::is_visible(hwnd) {
                    native::hide(hwnd);
                    hidden.push(hwnd.0 as isize);
                }
            }
        } else {
            let alive: Vec<isize> = self.launcher_hwnds().iter().map(|h| h.0 as isize).collect();
            for raw in hidden.drain(..) {
                if alive.contains(&raw) && !self.hidden_all.load(Ordering::SeqCst) {
                    native::show_no_activate(HWND(raw as _));
                }
            }
        }
    }
}
