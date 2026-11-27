//! Dispositions côté fenêtres : relever l'état actuel des versions web et de la barre,
//! et le remettre en place.

use super::{monitors, native, Widgets};
use bworkshop_core::dofus_tracker::SNAP_DISTANCE;
use bworkshop_core::geometry::{PerMonitor, Placement, Rect};
use bworkshop_core::layouts::{from_relative, to_relative, touches, Layout, LayoutWidget, Place};
use bworkshop_core::widgets_store::DEFAULT_WIDGET;
use std::collections::BTreeSet;
use tauri::{AppHandle, Manager};
use super::native::HWND;

/// Place d'une fenêtre : sur l'écran, et par rapport à Dofus si elle le touche.
fn place_of(rect: Rect, dofus: Option<Rect>, monitors: &[bworkshop_core::geometry::MonitorInfo]) -> Place {
    let mut screen = PerMonitor::default();
    screen.remember(rect, monitors);
    let dofus = dofus.filter(|d| touches(rect, *d, SNAP_DISTANCE as i32)).map(|d| to_relative(rect, d));
    Place { screen, dofus }
}

/// Rectangle où remettre une fenêtre (par rapport à Dofus si possible).
fn rect_of(place: &Place, dofus: Option<Rect>, monitors: &[bworkshop_core::geometry::MonitorInfo], default: Placement) -> Option<Rect> {
    match (place.dofus, dofus) {
        (Some(rel), Some(d)) => Some(from_relative(rel, d)),
        _ => place.screen.restore(monitors, default),
    }
}

impl Widgets {
    /// État actuel : versions web ouvertes et barre d'outils (si elle est ouverte).
    pub fn capture_layout(&self, app: &AppHandle, name: String, dofus: Option<Rect>) -> Layout {
        let monitors = monitors(app);
        let states = self.file.lock().map(|f| f.widgets.clone()).unwrap_or_default();
        let widgets = self
            .live_hwnds()
            .into_iter()
            .filter_map(|(id, hwnd)| {
                let rect = native::window_rect(hwnd)?;
                let st = states.get(&id);
                Some(LayoutWidget {
                    collapsed: st.is_some_and(|s| s.collapsed),
                    opacity: st.map(|s| s.opacity).unwrap_or(1.0),
                    place: place_of(rect, dofus, &monitors),
                    id,
                })
            })
            .collect();
        let toolbar = self.toolbar_rect().map(|r| place_of(r, dofus, &monitors));
        Layout { name, widgets, toolbar }
    }

    /// Remet une disposition en place. À appeler **hors du fil principal** : ouvrir une
    /// version web attend la création de sa page. Une barre fermée à l'enregistrement
    /// n'est pas fermée pour autant (elle sert à tout le reste).
    pub fn apply_layout(&self, app: &AppHandle, layout: &Layout, dofus: Option<Rect>) {
        let monitors = monitors(app);
        let wanted: BTreeSet<&str> = layout.widgets.iter().map(|w| w.id.as_str()).collect();
        let live: Vec<(String, HWND)> = self.live_hwnds();

        // 1. Fermer ce qui n'en fait pas partie.
        let to_close: Vec<String> = live.iter().map(|(id, _)| id.clone()).filter(|id| !wanted.contains(id.as_str())).collect();
        if !to_close.is_empty() {
            let app2 = app.clone();
            let _ = app.run_on_main_thread(move || {
                let w = app2.state::<Widgets>();
                for id in to_close {
                    w.close(&app2, &id);
                }
            });
        }

        // 2. Ouvrir ou replacer chaque version web.
        for lw in &layout.widgets {
            let rect = rect_of(&lw.place, dofus, &monitors, DEFAULT_WIDGET);
            if live.iter().any(|(id, _)| id == &lw.id) {
                let (app2, lw2) = (app.clone(), lw.clone());
                let _ = app.run_on_main_thread(move || {
                    let w = app2.state::<Widgets>();
                    w.set_opacity(&lw2.id, lw2.opacity);
                    w.emit_bar(&app2, &lw2.id);
                    w.set_collapsed(&app2, &lw2.id, lw2.collapsed);
                    if let (Some(hwnd), Some(r)) = (w.hwnd_of(&lw2.id), rect) {
                        native::set_rect(hwnd, r);
                        w.mark_moved(&lw2.id);
                    }
                });
            } else {
                if let Ok(mut f) = self.file.lock() {
                    let st = f.widget_mut(&lw.id);
                    st.opacity = bworkshop_core::widgets_store::clamp_opacity(lw.opacity);
                    st.collapsed = lw.collapsed;
                    if let Some(r) = rect {
                        if lw.collapsed {
                            st.pill.remember(r, &monitors);
                        } else {
                            st.window.remember(r, &monitors);
                        }
                    }
                }
                if let Err(e) = self.open(app, &lw.id) {
                    log::warn!("Disposition : version web {} non ouverte ({e})", lw.id);
                }
            }
        }

        // 3. La barre d'outils.
        if let Some(place) = &layout.toolbar {
            // Taille par défaut : celle d'une barre à une tuile (la vraie taille suit ensuite les tuiles).
            let tile = self.file.lock().map(|f| f.toolbar.tile_size()).unwrap_or(bworkshop_core::widgets_store::TILE_DEFAULT);
            let (w, h) = super::toolbar::size_for(1, false, false, tile);
            let rect = rect_of(place, dofus, &monitors, Placement { x: 40.0, y: 40.0, w, h });
            match (self.toolbar_hwnd_for_all(), rect) {
                (Some(hwnd), Some(r)) => {
                    let raw = hwnd.0 as isize;
                    let app2 = app.clone();
                    let _ = app.run_on_main_thread(move || {
                        // Garde la taille actuelle (elle suit le nombre de tuiles).
                        let size = native::window_rect(HWND(raw as _)).map(|c| (c.w, c.h)).unwrap_or((r.w, r.h));
                        native::set_rect(HWND(raw as _), Rect::new(r.x, r.y, size.0, size.1));
                        app2.state::<Widgets>().mark_moved(super::toolbar::MOVED_KEY);
                    });
                }
                (None, r) => {
                    if let (Some(r), Ok(mut f)) = (r, self.file.lock()) {
                        f.toolbar.position.remember(r, &monitors);
                    }
                    if let Err(e) = self.open_toolbar(app) {
                        log::warn!("Disposition : barre non ouverte ({e})");
                    }
                }
                _ => {}
            }
        }
        log::info!("Disposition rappelée : {} version(s) web", layout.widgets.len());
    }
}
