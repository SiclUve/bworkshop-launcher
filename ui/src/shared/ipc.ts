// Appels au cœur Rust. Les types reflètent `bworkshop_core` (settings, catalog).
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { getCurrentWebview } from '@tauri-apps/api/webview';

export type Lang = 'fr' | 'en';

export interface HotkeyBindings {
  togglePanel: string | null;
  toggleHideAll: string | null;
  toggleLock: string | null;
  layouts: (string | null)[];
  widgets: Record<string, string>;
}

/** Actions qui ont un raccourci. */
export type HotkeyAction = 'togglePanel' | 'toggleHideAll' | 'toggleLock';

/** État d'un raccourci : actif, ou refusé et pourquoi. */
export interface HotkeyStatus {
  /** Action du launcher, « layout:<n> » ou « tool:<id> ». */
  action: string;
  hotkey: string | null;
  active: boolean;
  error: string | null;
}
export const getHotkeysStatus = () => invoke<HotkeyStatus[]>('get_hotkeys_status');
export const suspendHotkeys = (suspended: boolean) => invoke<void>('suspend_hotkeys', { suspended });
export const onHotkeysStatus = (cb: (s: HotkeyStatus[]) => void): Promise<UnlistenFn> =>
  listen<HotkeyStatus[]>('hotkeys-status', (e) => cb(e.payload));

export interface Settings {
  version: number;
  language: Lang;
  autostart: boolean;
  dofusTitlePattern: string;
  /** Suivre la fenêtre de Dofus (désactivé par défaut). */
  followDofus: boolean;
  snapping: boolean;
  onlyWhenDofusForeground: boolean;
  reopenLastLayout: boolean;
  hotkeys: HotkeyBindings;
  favorites: string[];
  tutorialDone: boolean;
}

export interface AppInfo {
  version: string;
}

export interface Localized {
  fr: string;
  en: string;
}

export type ToolStatus = 'beta' | 'stable' | 'en-developpement';

export interface Tool {
  id: string;
  source: 'official' | 'community';
  name: string;
  author: string;
  category: string;
  kinds: string[];
  platforms: string[];
  status: ToolStatus;
  version: string | null;
  order: number;
  tagline: Localized | null;
  description: Localized | null;
  download: { url: string; sizeBytes: number | null; sha256: string | null } | null;
  webUrl: string | null;
  /** Version compacte à faire flotter sur le jeu (« Détacher en overlay »). */
  overlayUrl: string | null;
  iconUrl: string | null;
  pageUrl: string | null;
  sourceUrl: string | null;
  virustotalUrl: string | null;
  updatedAt: string | null;
  available: boolean;
}

export type SourceState =
  | { state: 'fresh' }
  | { state: 'cached'; reason: string; offline: boolean }
  | { state: 'unavailable'; reason: string };

export interface CatalogSnapshot {
  tools: Tool[];
  official: SourceState;
  community: SourceState;
  launcherUpdateRequired: boolean;
  fetchedAt: number | null;
  ignoredEntries: number;
}

export const getSettings = () => invoke<Settings>('get_settings');
export const saveSettings = (settings: Settings) => invoke<Settings>('save_settings', { settings });
export const appInfo = () => invoke<AppInfo>('app_info');

export const getCatalog = () => invoke<CatalogSnapshot | null>('get_catalog');
export const refreshCatalog = () => invoke<CatalogSnapshot | null>('refresh_catalog');
/** Icônes en cache : identifiant d'outil → adresse `data:`. */
export const getIcons = () => invoke<Record<string, string>>('get_icons');
export const onIconsUpdated = (cb: (icons: Record<string, string>) => void): Promise<UnlistenFn> =>
  listen<Record<string, string>>('icons-updated', (e) => cb(e.payload));
export const toggleFavorite = (id: string) => invoke<Settings>('toggle_favorite', { id });
export const openExternal = (url: string) => invoke<void>('open_external', { url });

// ---------- Installation ----------

export type InstallStatus =
  | { state: 'notInstalled' }
  | { state: 'installed'; version: string | null }
  | { state: 'updateAvailable'; installed: string; available: string };

export type Phase =
  | 'downloading'
  | 'verifying'
  | 'installing'
  | 'uninstalling'
  | 'downloaded'
  | 'done'
  | 'cancelled'
  | 'error';

export interface ProgressEvent {
  id: string;
  operation: 'install' | 'uninstall';
  phase: Phase;
  received: number;
  total: number | null;
  message: string | null;
}

export interface InstallsView {
  statuses: Record<string, InstallStatus>;
  activity: Record<string, ProgressEvent>;
}

export const getInstalls = () => invoke<InstallsView>('get_installs');
export const installTool = (id: string) => invoke<void>('install_tool', { id });
export const cancelInstall = (id: string) => invoke<void>('cancel_install', { id });
export const dismissInstallError = (id: string) => invoke<void>('dismiss_install_error', { id });
export const uninstallTool = (id: string) => invoke<void>('uninstall_tool', { id });
export const launchTool = (id: string) => invoke<void>('launch_tool', { id });
/** Logiciels lancés par le launcher ; `starting` : encore sans fenêtre. */
export interface RunningView {
  running: string[];
  starting: string[];
}
export const getRunning = () => invoke<RunningView>('get_running');
/** Fermeture polie (comme la croix), seulement pour un logiciel lancé par le launcher. */
export const closeTool = (id: string) => invoke<void>('close_tool', { id });
export const forceCloseTool = (id: string) => invoke<void>('force_close_tool', { id });
export const toolHasWindow = (id: string) => invoke<boolean>('tool_has_window', { id });
export const onRunningUpdated = (cb: (v: RunningView) => void): Promise<UnlistenFn> =>
  listen<RunningView>('running-updated', (e) => cb(e.payload));

export const onInstallProgress = (cb: (ev: ProgressEvent) => void): Promise<UnlistenFn> =>
  listen<ProgressEvent>('install-progress', (e) => cb(e.payload));
export const onInstallsUpdated = (cb: (view: InstallsView) => void): Promise<UnlistenFn> =>
  listen<InstallsView>('installs-updated', (e) => cb(e.payload));

export const onCatalogUpdated = (cb: (snap: CatalogSnapshot) => void): Promise<UnlistenFn> =>
  listen<CatalogSnapshot>('catalog-updated', (e) => cb(e.payload));

// ---------- Widgets ----------

export interface WidgetsView {
  open: string[];
  locked: boolean;
  hiddenAll: boolean;
  toolbarOpen: boolean;
}

export const getWidgets = () => invoke<WidgetsView>('get_widgets');
export const openWidget = (id: string) => invoke<void>('open_widget', { id });
export const closeWidget = (id: string) => invoke<void>('close_widget', { id });
export const setWidgetsLocked = (locked: boolean) => invoke<void>('set_widgets_locked', { locked });
export const setWidgetsHidden = (hidden: boolean) => invoke<void>('set_widgets_hidden', { hidden });
export const onWidgetsUpdated = (cb: (view: WidgetsView) => void): Promise<UnlistenFn> =>
  listen<WidgetsView>('widgets-updated', (e) => cb(e.payload));

// Barre d'un widget : agit seulement sur son propre widget.
export interface BarState {
  id: string;
  name: string;
  opacity: number;
  collapsed: boolean;
  locked: boolean;
  settingsOpen: boolean;
  language: Lang;
}

export const widgetBarState = () => invoke<BarState | null>('widget_bar_state');
export const widgetClose = () => invoke<void>('widget_close');
export const widgetToggleCollapse = () => invoke<void>('widget_toggle_collapse');
export const widgetReload = () => invoke<void>('widget_reload');
export const widgetSetOpacity = (value: number) => invoke<void>('widget_set_opacity', { value });
export const widgetSettings = (open: boolean) => invoke<void>('widget_settings', { open });
export const widgetDrag = () => invoke<void>('widget_drag');
export const onWidgetBarState = (cb: (s: BarState) => void): Promise<UnlistenFn> =>
  getCurrentWebview().listen<BarState>('widget-bar-state', (e) => cb(e.payload));

// ---------- Barre d'outils ----------

export const setToolbarOpen = (open: boolean) => invoke<void>('set_toolbar_open', { open });
export const toolbarFit = (count: number, message: boolean) => invoke<void>('toolbar_fit', { count, message });
export const toolbarMenu = (id: string) => invoke<void>('toolbar_menu', { id });
export const toolbarBarMenu = () => invoke<void>('toolbar_bar_menu');
/** Comment ouvrir un outil : logiciel installé, overlay web (compact) ou page web complète. */
export type OpenMode = 'app' | 'overlay' | 'web';
/** Réglages de la barre : tuiles retirées ou ajoutées, mode par outil, ordre, orientation. */
export interface ToolbarPrefs {
  hidden: string[];
  shown: string[];
  modes: Record<string, OpenMode>;
  order: string[];
  vertical: boolean;
  /** Taille des tuiles en pixels (28 à 112), choisie en tirant un coin de la barre. */
  tile: number;
}
/** Même règle que `ToolbarState::in_bar` : installé sauf retiré, web seulement si ajouté. */
export const inBar = (p: ToolbarPrefs, id: string, installed: boolean) =>
  !p.hidden.includes(id) && (installed || p.shown.includes(id));
/** Modes possibles pour un outil, dans l'ordre de préférence. */
export const availableModes = (t: Tool, installed: boolean): OpenMode[] => {
  const m: OpenMode[] = [];
  if (installed) m.push('app');
  if (t.overlayUrl) m.push('overlay');
  if (t.webUrl) m.push('web');
  return m;
};
/** Même règle que `effective_mode` : le mode choisi s'il existe, sinon le premier possible. */
export const effectiveMode = (p: ToolbarPrefs, t: Tool, installed: boolean): OpenMode | null => {
  const modes = availableModes(t, installed);
  const chosen = p.modes[t.id];
  return chosen && modes.includes(chosen) ? chosen : (modes[0] ?? null);
};
export const getToolbarPrefs = () => invoke<ToolbarPrefs>('get_toolbar_prefs');
export const setToolInBar = (id: string, value: boolean) => invoke<void>('set_tool_in_bar', { id, value });
export const setToolMode = (id: string, mode: OpenMode | null) => invoke<void>('set_tool_mode', { id, mode });
/** Ouvre ou ferme un outil selon son mode. */
export const toggleTool = (id: string) => invoke<void>('toggle_tool', { id });
export const toolbarPrefs = () => invoke<ToolbarPrefs>('toolbar_prefs');
export const toolbarSetOrder = (order: string[]) => invoke<void>('toolbar_set_order', { order });
/** Coin tiré pour changer la taille des tuiles (le coin opposé reste en place). */
export type Corner = 'topLeft' | 'topRight' | 'bottomLeft' | 'bottomRight';
export const toolbarResize = (tile: number, count: number, message: boolean, corner: Corner, done: boolean) =>
  invoke<void>('toolbar_resize', { tile, count, message, corner, done });
export const onToolbarPrefs = (cb: (p: ToolbarPrefs) => void): Promise<UnlistenFn> =>
  listen<ToolbarPrefs>('toolbar-prefs', (e) => cb(e.payload));
export const onToolbarNotice = (cb: (message: string) => void): Promise<UnlistenFn> =>
  listen<string>('toolbar-notice', (e) => cb(e.payload));
export const toolbarDrag = () => invoke<void>('toolbar_drag');
/** Bascule la fenêtre principale ; renvoie true si elle est maintenant ouverte. */
export const toolbarToggleMain = () => invoke<boolean>('toolbar_open_main');
export const toolbarMainVisible = () => invoke<boolean>('toolbar_main_visible');
export const onMainVisible = (cb: (visible: boolean) => void): Promise<UnlistenFn> =>
  listen<boolean>('main-visible', (e) => cb(e.payload));
export const onSettingsUpdated = (cb: (s: Settings) => void): Promise<UnlistenFn> =>
  listen<Settings>('settings-updated', (e) => cb(e.payload));

// ---------- Dispositions ----------

export interface LayoutInfo {
  name: string;
  /** Nombre de versions web. */
  widgets: number;
  /** Leurs identifiants d'outils. */
  tools: string[];
  toolbar: boolean;
  /** Au moins une fenêtre placée par rapport à Dofus. */
  followsDofus: boolean;
}
/** Élément « barre d'outils » pour `layoutRemoveItem`. */
export const LAYOUT_TOOLBAR = ':toolbar';
export interface LayoutsView {
  layouts: LayoutInfo[];
  active: number | null;
}
export const getLayouts = () => invoke<LayoutsView>('get_layouts');
export const layoutSave = (name: string) => invoke<LayoutsView>('layout_save', { name });
export const layoutOverwrite = (index: number) => invoke<LayoutsView>('layout_overwrite', { index });
export const layoutRename = (index: number, name: string) => invoke<LayoutsView>('layout_rename', { index, name });
export const layoutMove = (from: number, to: number) => invoke<LayoutsView>('layout_move', { from, to });
export const layoutDelete = (index: number) => invoke<LayoutsView>('layout_delete', { index });
export const layoutApply = (index: number) => invoke<void>('layout_apply', { index });
/** Retire un outil (ou la barre : `LAYOUT_TOOLBAR`) d'une disposition. */
export const layoutRemoveItem = (index: number, item: string) =>
  invoke<LayoutsView>('layout_remove_item', { index, item });
export const takeLayoutsRequest = () => invoke<boolean>('take_layouts_request');
export const onLayoutsUpdated = (cb: (view: LayoutsView) => void): Promise<UnlistenFn> =>
  listen<LayoutsView>('layouts-updated', (e) => cb(e.payload));
export const onOpenLayouts = (cb: () => void): Promise<UnlistenFn> => listen('open-layouts', () => cb());

// ---------- Mise à jour du launcher ----------

export interface LauncherUpdate {
  version: string;
  notes: string | null;
}
export const getLauncherUpdate = () => invoke<LauncherUpdate | null>('get_launcher_update');
export const installLauncherUpdate = () => invoke<void>('install_launcher_update');
export const onLauncherUpdate = (cb: (u: LauncherUpdate) => void): Promise<UnlistenFn> =>
  listen<LauncherUpdate>('launcher-update', (e) => cb(e.payload));
export const onLauncherUpdateProgress = (cb: (percent: number | null) => void): Promise<UnlistenFn> =>
  listen<number | null>('launcher-update-progress', (e) => cb(e.payload));
