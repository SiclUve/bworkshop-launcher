<script lang="ts">
  // Barre d'outils : une tuile par outil choisi dans « Mes plugins » (logiciel installé,
  // overlay web ou page web selon son mode), puis « + ».
  // Dimensions à garder en phase avec `widgets/toolbar.rs` (TILE, GAP, PAD, HANDLE, MESSAGE_*).
  import { onMount } from 'svelte';
  import {
    getCatalog,
    getIcons,
    getInstalls,
    getSettings,
    launchTool,
    closeTool,
    forceCloseTool,
    toolHasWindow,
    toolbarMainVisible,
    onMainVisible,
    getRunning,
    onRunningUpdated,
    onCatalogUpdated,
    onIconsUpdated,
    onInstallsUpdated,
    onToolbarNotice,
    toolbarDrag,
    toolbarFit,
    toolbarMenu,
    toolbarBarMenu,
    toolbarPrefs,
    toolbarSetOrder,
    onToolbarPrefs,
    toolbarToggleMain,
    toggleTool,
    getWidgets,
    onWidgetsUpdated,
    inBar,
    effectiveMode,
    type OpenMode,
    type CatalogSnapshot,
    type InstallsView,
    type Lang,
    type RunningView,
    type Tool,
    type ToolbarPrefs,
  } from '../shared/ipc';
  import { t } from '../shared/i18n';

  let catalog = $state<CatalogSnapshot | null>(null);
  let installs = $state<InstallsView>({ statuses: {}, activity: {} });
  let icons = $state<Record<string, string>>({});
  let favorites = $state<string[]>([]);
  // Lancés par le launcher ; « starting » : encore sans fenêtre (tuile qui pulse).
  let running = $state<string[]>([]);
  let starting = $state<string[]>([]);
  // Logiciels qui tournent encore 3 s après la fermeture polie : tuile rouge, prochain clic = arrêt forcé.
  // Au bout de 60 s (comme côté Rust), la tuile redevient normale : un clic redemande une fermeture polie
  // (le logiciel a pu proposer « Enregistrer ? » et l'utilisateur annuler).
  let stubborn = $state<string[]>([]);
  let mainVisible = $state(false);
  const FORCE_AFTER_MS = 3000;
  const FORCE_WINDOW_MS = 60000;
  let lang = $state<Lang>('fr');

  // Message court sous les tuiles (erreur ou conseil), effacé tout seul.
  let notice = $state('');
  const NOTICE_MS = 7000;
  let noticeTimer: ReturnType<typeof setTimeout> | undefined;
  function showNotice(message: string) {
    notice = message.replace(/^Error:\s*/, '');
    clearTimeout(noticeTimer);
    noticeTimer = setTimeout(() => (notice = ''), NOTICE_MS);
  }
  function hideNotice() {
    clearTimeout(noticeTimer);
    notice = '';
  }

  // Tuiles retirées, ordre choisi en glissant, orientation.
  let prefs = $state<ToolbarPrefs>({ hidden: [], shown: [], modes: {}, order: [], vertical: false });
  // Versions web ouvertes sur le jeu (tuile allumée).
  let webOpen = $state<string[]>([]);

  const isInstalled = (id: string) => (installs.statuses[id]?.state ?? 'notInstalled') !== 'notInstalled';
  const modeOf = (tool: Tool): OpenMode | null => effectiveMode(prefs, tool, isInstalled(tool.id));
  const isOn = (tool: Tool) => (modeOf(tool) === 'app' ? running.includes(tool.id) : webOpen.includes(tool.id));

  // Tuiles affichées (nom gardé pour l'historique : installés ET outils web ajoutés) :
  // d'abord l'ordre choisi, puis favoris, puis ordre du catalogue.
  const installed = $derived.by(() => {
    const tools = (catalog?.tools ?? []).filter((x) => inBar(prefs, x.id, isInstalled(x.id)) && modeOf(x) !== null);
    const rank = (id: string) => {
      const i = prefs.order.indexOf(id);
      return i === -1 ? Number.MAX_SAFE_INTEGER : i;
    };
    return tools.sort((a, b) => {
      const fa = favorites.includes(a.id) ? 0 : 1;
      const fb = favorites.includes(b.id) ? 0 : 1;
      return rank(a.id) - rank(b.id) || fa - fb || a.order - b.order || a.name.localeCompare(b.name);
    });
  });
  const nameOf = (id: string) => catalog?.tools.find((x) => x.id === id)?.name ?? id;

  // Disposition de la grille (6 tuiles au plus par ligne, ou par colonne si verticale).
  const MAX_PER_LINE = 6;
  const gridStyle = $derived.by(() => {
    const n = installed.length + 1;
    if (prefs.vertical) {
      const rows = Math.min(n, MAX_PER_LINE);
      return `grid-auto-flow: column; grid-template-rows: repeat(${rows}, 44px); grid-template-columns: none;`;
    }
    return `grid-template-columns: repeat(${Math.min(n, MAX_PER_LINE)}, 44px);`;
  });

  // La fenêtre suit le nombre de tuiles (« + » comprise), l'orientation, et s'agrandit pour un message.
  $effect(() => {
    void prefs.vertical;
    toolbarFit(installed.length + 1, notice !== '');
  });

  // ---------- Glisser une tuile pour changer l'ordre ----------
  const DRAG_THRESHOLD = 6;
  let drag = $state<{ id: string; x: number; y: number; active: boolean } | null>(null);
  let overId = $state<string | null>(null);
  // Un glisser se termine par un « click » : on l'ignore.
  let suppressClick = false;

  function onTileDown(e: PointerEvent, tool: Tool) {
    if (e.button !== 0) return;
    drag = { id: tool.id, x: e.clientX, y: e.clientY, active: false };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }
  function onTileMove(e: PointerEvent) {
    if (!drag) return;
    if (!drag.active && Math.hypot(e.clientX - drag.x, e.clientY - drag.y) > DRAG_THRESHOLD) drag.active = true;
    if (!drag.active) return;
    const el = document.elementFromPoint(e.clientX, e.clientY)?.closest<HTMLElement>('[data-tool]');
    overId = el?.dataset.tool ?? null;
  }
  function onTileUp() {
    if (drag?.active) {
      suppressClick = true;
      if (overId && overId !== drag.id) reorder(drag.id, overId);
    }
    drag = null;
    overId = null;
  }
  function reorder(moved: string, target: string) {
    const ids = installed.map((x) => x.id).filter((id) => id !== moved);
    ids.splice(ids.indexOf(target) + (installed.findIndex((x) => x.id === moved) < installed.findIndex((x) => x.id === target) ? 1 : 0), 0, moved);
    // Les outils retirés gardent leur place dans l'ordre enregistré.
    const order = [...ids, ...prefs.order.filter((id) => !ids.includes(id))];
    prefs = { ...prefs, order };
    toolbarSetOrder(order).catch((err) => showNotice(String(err)));
  }

  const hasUpdate = (tool: Tool) => installs.statuses[tool.id]?.state === 'updateAvailable';
  const tileTitle = (tool: Tool) => {
    const action = tileLabel(tool);
    return hasUpdate(tool) ? `${action} · ${t(lang, 'toolbar.update')}` : action;
  };
  function tileLabel(tool: Tool) {
    if (modeOf(tool) !== 'app') {
      const what = t(lang, modeOf(tool) === 'overlay' ? 'mode.overlay' : 'mode.web');
      return t(lang, webOpen.includes(tool.id) ? 'toolbar.closeWeb' : 'toolbar.openWeb', { name: tool.name, what });
    }
    const key = starting.includes(tool.id)
      ? 'toolbar.starting'
      : stubborn.includes(tool.id)
        ? 'toolbar.force'
        : running.includes(tool.id)
          ? 'toolbar.close'
          : 'toolbar.launch';
    return t(lang, key, { name: tool.name });
  }
  async function onToggleMain() {
    try {
      mainVisible = await toolbarToggleMain();
    } catch (e) {
      showNotice(String(e));
    }
  }
  const initial = (tool: Tool) => tool.name.trim().charAt(0).toUpperCase();

  function applyRunning(v: RunningView) {
    // Parti avant d'avoir montré une fenêtre : souvent déjà ouvert ailleurs (une seule copie permise).
    const vanished = starting.filter((id) => !v.running.includes(id));
    if (vanished.length > 0) showNotice(t(lang, 'toolbar.quickExit', { name: nameOf(vanished[0]) }));
    running = v.running;
    starting = v.starting;
    stubborn = stubborn.filter((id) => v.running.includes(id));
  }

  async function reload() {
    try {
      const [c, i, ic, s, r, m, p, w] = await Promise.all([
        getCatalog(),
        getInstalls(),
        getIcons(),
        getSettings(),
        getRunning(),
        toolbarMainVisible(),
        toolbarPrefs(),
        getWidgets(),
      ]);
      prefs = p;
      webOpen = w.open;
      running = r.running;
      starting = r.starting;
      mainVisible = m;
      catalog = c;
      installs = i;
      icons = ic;
      favorites = s.favorites;
      lang = s.language;
      document.documentElement.lang = lang;
    } catch (e) {
      showNotice(String(e));
    }
  }

  onMount(() => {
    reload();
    // Pas de menu « Précédent / Actualiser / Inspecter » du navigateur intégré.
    const noMenu = (e: MouseEvent) => e.preventDefault();
    document.addEventListener('contextmenu', noMenu);
    const offs = [
      onCatalogUpdated(() => reload()),
      onInstallsUpdated((v) => (installs = v)),
      onIconsUpdated((m) => (icons = m)),
      onRunningUpdated(applyRunning),
      onMainVisible((v) => (mainVisible = v)),
      onToolbarNotice(showNotice),
      onToolbarPrefs((p) => (prefs = p)),
      onWidgetsUpdated((w) => (webOpen = w.open)),
    ];
    return () => {
      document.removeEventListener('contextmenu', noMenu);
      clearTimeout(noticeTimer);
      offs.forEach((o) => o.then((f) => f()));
    };
  });

  // Éteint : lancer. Allumé : demander poliment la fermeture.
  // Rouge (toujours là 3 s après) : arrêt forcé de ce que le launcher a lancé.
  // En démarrage : on ne relance pas (évite de lancer deux fois), on le dit seulement.
  async function onLaunch(tool: Tool) {
    const id = tool.id;
    if (suppressClick) {
      suppressClick = false;
      return;
    }
    if (starting.includes(id)) {
      showNotice(t(lang, 'toolbar.starting', { name: tool.name }));
      return;
    }
    hideNotice();
    // Overlay ou page web : ouvrir ou fermer sa fenêtre sur le jeu.
    if (modeOf(tool) !== 'app') {
      try {
        await toggleTool(id);
      } catch (e) {
        showNotice(String(e));
      }
      return;
    }
    try {
      if (stubborn.includes(id)) {
        await forceCloseTool(id);
      } else if (running.includes(id)) {
        await closeTool(id);
        setTimeout(() => (stubborn = stubborn.filter((x) => x !== id)), FORCE_WINDOW_MS);
        setTimeout(async () => {
          if (!running.includes(id) || stubborn.includes(id)) return;
          stubborn = [...stubborn, id];
          // Fenêtre disparue mais logiciel actif : il s'est caché, il n'est pas bloqué.
          const visible = await toolHasWindow(id).catch(() => true);
          showNotice(t(lang, visible ? 'toolbar.stubbornHint' : 'toolbar.hiddenHint', { name: tool.name }));
        }, FORCE_AFTER_MS);
      } else {
        await launchTool(id);
      }
    } catch (e) {
      showNotice(String(e));
    }
  }

  async function onMenu(e: MouseEvent, tool: Tool) {
    e.preventDefault();
    try {
      await toolbarMenu(tool.id);
    } catch (err) {
      showNotice(String(err));
    }
  }

  async function onBarMenu(e: MouseEvent) {
    e.preventDefault();
    try {
      await toolbarBarMenu();
    } catch (err) {
      showNotice(String(err));
    }
  }

  function onHandleDown(e: PointerEvent) {
    if (e.button === 0) toolbarDrag();
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="handle" onpointerdown={onHandleDown} title={t(lang, 'toolbar.dragHint')}>
  <span class="grip" aria-hidden="true"></span>
</div>

<ul class="grid" style={gridStyle} aria-label={t(lang, 'toolbar.label')}>
  {#each installed as tool (tool.id)}
    <li data-tool={tool.id}>
      <button
        type="button"
        class="tile"
        class:dragging={drag?.active && drag.id === tool.id}
        class:drop-target={drag?.active && overId === tool.id && drag.id !== tool.id}
        onpointerdown={(e) => onTileDown(e, tool)}
        onpointermove={onTileMove}
        onpointerup={onTileUp}
        onpointercancel={onTileUp}
        class:running={isOn(tool)}
        class:web={modeOf(tool) !== 'app'}
        class:starting={starting.includes(tool.id)}
        class:stubborn={stubborn.includes(tool.id)}
        aria-pressed={isOn(tool)}
        aria-busy={starting.includes(tool.id)}
        title={tileTitle(tool)}
        aria-label={tileLabel(tool)}
        onclick={() => onLaunch(tool)}
        oncontextmenu={(e) => onMenu(e, tool)}
      >
        {#if icons[tool.id]}
          <img src={icons[tool.id]} alt="" />
        {:else}
          <span class="initial" aria-hidden="true">{initial(tool)}</span>
        {/if}
        {#if hasUpdate(tool)}<span class="badge" aria-hidden="true"></span>{/if}
        {#if modeOf(tool) !== 'app'}<span class="web-mark" aria-hidden="true">web</span>{/if}
      </button>
    </li>
  {/each}
  <li>
    <button
      type="button"
      class="tile add"
      aria-pressed={mainVisible}
      title={t(lang, mainVisible ? 'toolbar.hideMain' : 'toolbar.add')}
      aria-label={t(lang, mainVisible ? 'toolbar.hideMain' : 'toolbar.add')}
      onclick={onToggleMain}
      oncontextmenu={onBarMenu}
    >
      <svg viewBox="0 0 20 20" aria-hidden="true"><path d={mainVisible ? 'M4 10h12' : 'M10 4v12M4 10h12'} /></svg>
    </button>
  </li>
</ul>

{#if notice}
  <div class="notice" role="status">
    <p>{notice}</p>
    <button type="button" class="dismiss" aria-label={t(lang, 'toolbar.dismiss')} title={t(lang, 'toolbar.dismiss')} onclick={hideNotice}>×</button>
  </div>
{/if}

<style>
  :global(body.toolbar) {
    overflow: hidden;
    user-select: none;
    background: #141811;
  }

  .handle {
    height: 14px;
    display: flex;
    align-items: center;
    justify-content: center;
    position: relative;
    cursor: grab;
    /* Planche cloutée, comme les séparateurs du site. */
    background:
      radial-gradient(circle at 7px 50%, #e2d8bf 0 1.4px, #3d3a33 1.9px 2.8px, transparent 3.2px),
      radial-gradient(circle at calc(100% - 7px) 50%, #e2d8bf 0 1.4px, #3d3a33 1.9px 2.8px, transparent 3.2px),
      linear-gradient(#a8743d, #7a4f27 55%, #5c3a1c);
  }

  .grip {
    width: 28px;
    height: 4px;
    border-radius: 2px;
    background: rgba(233, 220, 192, 0.5);
  }

  .grid {
    list-style: none;
    margin: 0;
    padding: 6px;
    display: grid;
    grid-template-columns: repeat(auto-fill, 44px);
    gap: 4px;
  }

  .tile {
    position: relative;
    width: 44px;
    height: 44px;
    padding: 3px;
    border: 1px solid var(--line);
    border-radius: 0;
    background: linear-gradient(#343025, #25251e);
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.08), 0 1px 2px rgba(0, 0, 0, 0.6);
    display: grid;
    place-items: center;
  }

  .tile:hover {
    border-color: var(--accent);
    background: linear-gradient(#433b2c, #2c2a21);
  }

  /* Logiciel ouvert : cadre doré lumineux. */
  .tile.running {
    border-color: var(--accent);
    box-shadow: 0 0 0 1px var(--accent), 0 0 8px rgba(239, 189, 103, 0.55);
  }

  /* Ne répond pas à la fermeture : rouge, le prochain clic force l'arrêt. */
  .tile.running.stubborn {
    border-color: #e0614a;
    box-shadow: 0 0 0 1px #e0614a, 0 0 8px rgba(224, 97, 74, 0.65);
  }

  .tile:active {
    transform: translateY(1px);
  }

  .tile:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: 1px;
  }

  .tile img {
    width: 100%;
    height: 100%;
    object-fit: contain;
    pointer-events: none;
  }

  .initial {
    font-family: var(--font-paint);
    font-size: 22px;
    color: var(--text);
  }

  .badge {
    position: absolute;
    top: 2px;
    right: 2px;
    width: 9px;
    height: 9px;
    border-radius: 50%;
    background: var(--accent);
    border: 1px solid #1f241b;
  }

  /* Petite étiquette « web » : la tuile ouvre une fenêtre web, pas un logiciel. */
  .web-mark {
    position: absolute;
    bottom: 1px;
    left: 50%;
    transform: translateX(-50%);
    padding: 0 3px;
    border-radius: 0;
    font-size: 8px;
    line-height: 10px;
    font-weight: 700;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: #1f241b;
    background: rgba(233, 220, 192, 0.85);
    pointer-events: none;
  }

  .add svg {
    width: 20px;
    height: 20px;
    stroke: var(--text-muted);
    stroke-width: 2;
    stroke-linecap: round;
  }

  .add:hover svg {
    stroke: var(--text);
  }

  /* Glisser pour réordonner : la tuile tenue s'estompe, la place visée s'éclaire. */
  .tile.dragging {
    opacity: 0.45;
    cursor: grabbing;
  }

  .tile.drop-target {
    outline: 2px dashed var(--accent);
    outline-offset: 1px;
  }

  /* En démarrage : cadre doré qui pulse, jusqu'à l'apparition de la fenêtre. */
  .tile.starting {
    animation: pulse 1.1s ease-in-out infinite;
    cursor: progress;
  }

  .tile.starting img,
  .tile.starting .initial {
    opacity: 0.6;
  }

  @keyframes pulse {
    0%,
    100% {
      box-shadow: 0 0 0 1px rgba(227, 180, 90, 0.35);
    }
    50% {
      box-shadow: 0 0 0 1px var(--accent), 0 0 10px rgba(227, 180, 90, 0.7);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .tile.starting {
      animation: none;
      box-shadow: 0 0 0 1px var(--accent);
    }
  }

  /* Bande de message (40 px, voir MESSAGE_HEIGHT dans toolbar.rs). */
  .notice {
    height: 34px;
    margin: 0 6px 6px;
    padding: 0 2px 0 8px;
    display: flex;
    align-items: center;
    gap: 4px;
    border: 1px solid #6b5536;
    border-radius: 0;
    background: #2a2418;
  }

  .notice p {
    flex: 1;
    margin: 0;
    font-size: 12px;
    line-height: 15px;
    color: var(--parchment);
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  .dismiss {
    flex: none;
    width: 24px;
    height: 24px;
    border: 0;
    border-radius: 0;
    background: transparent;
    color: var(--text-muted);
    font-size: 16px;
    line-height: 1;
  }

  .dismiss:hover {
    color: var(--text);
    background: rgba(255, 255, 255, 0.08);
  }

  .dismiss:focus-visible {
    outline: 2px solid var(--focus);
  }
</style>
