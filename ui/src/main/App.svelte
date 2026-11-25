<script lang="ts">
  import { onMount } from 'svelte';
  import logo from '../shared/logo.png';
  import bworkHead from '../shared/bw/bwork-head.webp';
  import {
    appInfo,
    cancelInstall,
    dismissInstallError,
    getCatalog,
    getInstalls,
    getSettings,
    installTool,
    launchTool,
    onCatalogUpdated,
    onInstallProgress,
    onInstallsUpdated,
    openExternal,
    refreshCatalog,
    saveSettings,
    toggleFavorite,
    uninstallTool,
    UNVERIFIED,
    closeWidget,
    getWidgets,
    getIcons,
    onIconsUpdated,
    onWidgetsUpdated,
    openWidget,
    setWidgetsHidden,
    setWidgetsLocked,
    setToolbarOpen,
    getLayouts,
    onLayoutsUpdated,
    onOpenLayouts,
    takeLayoutsRequest,
    type LayoutsView,
    getLauncherUpdate,
    installLauncherUpdate,
    onLauncherUpdate,
    onLauncherUpdateProgress,
    type LauncherUpdate,
    type WidgetsView,
    type CatalogSnapshot,
    type InstallStatus,
    type InstallsView,
    type Lang,
    type Settings,
    type Tool,
  } from '../shared/ipc';
  import { t } from '../shared/i18n';
  import { search } from '../shared/fuzzy';
  import ToolCard from './ToolCard.svelte';
  import Shortcuts from './Shortcuts.svelte';
  import Tour from './Tour.svelte';
  import SettingsDialog from './Settings.svelte';
  import LayoutsDialog from './Layouts.svelte';
  import Plugins from './Plugins.svelte';

  type Tab = 'official' | 'community' | 'installed' | 'plugins';
  const tabs: Tab[] = ['official', 'community', 'installed', 'plugins'];
  // Icône dessinée (planche du site) de chaque onglet.
  const tabIcon: Record<Tab, string> = { official: 'tools', community: 'chat', installed: 'done', plugins: 'all' };

  let settings = $state<Settings | null>(null);
  let catalog = $state<CatalogSnapshot | null>(null);
  let version = $state('');
  let tab = $state<Tab>('official');
  let query = $state('');
  let category = $state('');
  let refreshing = $state(false);
  let error = $state('');
  let installs = $state<InstallsView>({ statuses: {}, activity: {} });
  // Outils dont l'installation attend la confirmation « sans empreinte publiée ».
  let unverified = $state<string[]>([]);
  let icons = $state<Record<string, string>>({});
  let widgets = $state<WidgetsView>({ open: [], locked: false, hiddenAll: false, toolbarOpen: false });

  let showShortcuts = $state(false);
  let showSettings = $state(false);
  let showLayouts = $state(false);
  let launcherUpdate = $state<LauncherUpdate | null>(null);
  // null : pas en cours ; -1 : en cours sans pourcentage connu.
  let updateProgress = $state<number | null>(null);
  let layouts = $state<LayoutsView>({ layouts: [], active: null });
  const activeLayout = $derived(layouts.active !== null ? layouts.layouts[layouts.active] : undefined);
  // Visite guidée : d'elle-même au premier lancement, puis par le bouton « Aide ».
  let replayTour = $state(false);
  const showTour = $derived(!!settings && (replayTour || !settings.tutorialDone));
  async function onTourDone() {
    replayTour = false;
    if (settings && !settings.tutorialDone) await update({ tutorialDone: true });
  }
  // Outils qui peuvent avoir un raccourci : installés, ou avec une version web.
  const shortcutTools = $derived(
    (catalog?.tools ?? [])
      .filter((x) => (installs.statuses[x.id]?.state ?? 'notInstalled') !== 'notInstalled' || !!x.webUrl)
      .map((x) => ({ id: x.id, name: x.name })),
  );

  const lang = $derived<Lang>(settings?.language ?? 'fr');
  const favorites = $derived(settings?.favorites ?? []);
  const NOT_INSTALLED: InstallStatus = { state: 'notInstalled' };
  const statusOf = (id: string): InstallStatus => installs.statuses[id] ?? NOT_INSTALLED;
  const isInstalled = (id: string) => statusOf(id).state !== 'notInstalled';
  const updatable = $derived(
    (catalog?.tools ?? []).filter((x) => statusOf(x.id).state === 'updateAvailable' && x.available),
  );

  // Outils de l'onglet : favoris d'abord, puis l'ordre du catalogue.
  const tabTools = $derived.by(() => {
    const all = catalog?.tools ?? [];
    if (tab === 'plugins') return [];
    const inTab = tab === 'installed' ? all.filter((x) => isInstalled(x.id)) : all.filter((x) => x.source === tab);
    return [...inTab].sort((a, b) => {
      const fa = favorites.includes(a.id) ? 0 : 1;
      const fb = favorites.includes(b.id) ? 0 : 1;
      return fa - fb || a.order - b.order || a.name.localeCompare(b.name);
    });
  });

  const categories = $derived([...new Set(tabTools.map((x) => x.category))].sort());

  const visibleTools = $derived(
    search(
      query,
      tabTools.filter((x) => !category || x.category === category),
      (x: Tool) => [x.name, x.author, x.category, x.tagline?.[lang] ?? ''],
    ),
  );

  // Bandeau d'état du catalogue.
  const banner = $derived.by(() => {
    if (!catalog) return '';
    if (catalog.launcherUpdateRequired) return t(lang, 'catalog.updateRequired');
    const states = [catalog.official, catalog.community];
    if (states.every((s) => s.state === 'unavailable')) return t(lang, 'catalog.unavailable');
    if (states.some((s) => s.state === 'cached' && s.reason.startsWith('hors ligne')) && catalog.fetchedAt) {
      const date = new Date(catalog.fetchedAt * 1000).toLocaleString(lang === 'fr' ? 'fr-FR' : 'en-GB');
      return t(lang, 'catalog.offline', { date });
    }
    return '';
  });

  async function reloadInstalls() {
    try {
      installs = await getInstalls();
    } catch (e) {
      error = String(e);
    }
  }

  onMount(() => {
    const unlisteners = [
      onCatalogUpdated((snap) => {
        catalog = snap;
        getIcons().then((map) => (icons = map));
        reloadInstalls();
      }),
      onInstallsUpdated((view) => (installs = view)),
      onWidgetsUpdated((view) => (widgets = view)),
      onIconsUpdated((map) => (icons = map)),
      onLayoutsUpdated((view) => (layouts = view)),
      onOpenLayouts(() => (showLayouts = true)),
      onLauncherUpdate((u) => (launcherUpdate = u)),
      onLauncherUpdateProgress((p) => (updateProgress = p ?? -1)),
      onInstallProgress((ev) => {
        const activity = { ...installs.activity };
        if (ev.phase === 'done' || ev.phase === 'cancelled' || ev.phase === 'downloaded') delete activity[ev.id];
        else activity[ev.id] = ev;
        installs = { ...installs, activity };
      }),
    ];
    (async () => {
      try {
        const [s, info, c] = await Promise.all([getSettings(), appInfo(), getCatalog()]);
        settings = s;
        version = info.version;
        catalog = c ?? catalog;
        widgets = await getWidgets();
        icons = await getIcons();
        layouts = await getLayouts();
        launcherUpdate = await getLauncherUpdate();
        if (await takeLayoutsRequest()) showLayouts = true;
        await reloadInstalls();
      } catch (e) {
        error = `${t(lang, 'error.load')} : ${e}`;
      }
    })();
    return () => {
      unlisteners.forEach((u) => u.then((f) => f()));
    };
  });

  async function onInstall(id: string, acceptUnverified: boolean) {
    try {
      await installTool(id, acceptUnverified);
      unverified = unverified.filter((x) => x !== id);
    } catch (e) {
      if (String(e) === UNVERIFIED) {
        if (!unverified.includes(id)) unverified = [...unverified, id];
      } else {
        error = String(e);
      }
    }
  }

  async function onUpdateAll() {
    // Les outils sans empreinte publiée restent à confirmer un par un.
    for (const tool of updatable) await onInstall(tool.id, false);
  }

  async function onCancel(id: string) {
    await cancelInstall(id).catch((e) => (error = String(e)));
  }

  async function onUninstall(id: string) {
    await uninstallTool(id).catch((e) => (error = String(e)));
  }

  async function onLaunch(id: string) {
    await launchTool(id).catch((e) => (error = String(e)));
  }

  async function onDismiss(id: string) {
    unverified = unverified.filter((x) => x !== id);
    const activity = { ...installs.activity };
    delete activity[id];
    installs = { ...installs, activity };
    await dismissInstallError(id).catch(() => {});
  }

  async function onWidget(id: string, open: boolean) {
    try {
      if (open) await openWidget(id);
      else await closeWidget(id);
      widgets = await getWidgets();
    } catch (e) {
      error = String(e);
    }
  }

  async function onInstallLauncherUpdate() {
    updateProgress = -1;
    try {
      await installLauncherUpdate();
    } catch (e) {
      updateProgress = null;
      error = String(e);
    }
  }

  async function onLockWidgets(locked: boolean) {
    await setWidgetsLocked(locked).catch((e) => (error = String(e)));
  }

  async function onToggleToolbar() {
    await setToolbarOpen(!widgets.toolbarOpen).catch((e) => (error = String(e)));
    widgets = await getWidgets();
  }

  async function onHideWidgets() {
    await setWidgetsHidden(!widgets.hiddenAll).catch((e) => (error = String(e)));
  }

  async function update(patch: Partial<Settings>) {
    if (!settings) return;
    try {
      settings = await saveSettings({ ...settings, ...patch });
      error = '';
    } catch (e) {
      error = String(e);
    }
  }

  async function onRefresh() {
    refreshing = true;
    try {
      catalog = (await refreshCatalog()) ?? catalog;
      await reloadInstalls();
    } catch (e) {
      error = String(e);
    } finally {
      refreshing = false;
    }
  }

  async function onToggleFavorite(id: string) {
    try {
      settings = await toggleFavorite(id);
    } catch (e) {
      error = String(e);
    }
  }

  async function onOpenLink(url: string) {
    try {
      await openExternal(url);
    } catch (e) {
      error = String(e);
    }
  }

  // Navigation entre onglets au clavier (flèches gauche/droite).
  function onTabKey(event: KeyboardEvent) {
    const i = tabs.indexOf(tab);
    if (event.key === 'ArrowRight') tab = tabs[(i + 1) % tabs.length];
    else if (event.key === 'ArrowLeft') tab = tabs[(i + tabs.length - 1) % tabs.length];
    else return;
    event.preventDefault();
    category = '';
    document.getElementById(`tab-${tab}`)?.focus();
  }
</script>

<main>
  <header>
    <img class="logo" src={logo} alt="" width="44" height="44" />
    <div class="brand">
      <h1>Bworkshop</h1>
      <span class="kicker">{t(lang, 'app.subtitle')}</span>
    </div>
    <button class="btn-gold" aria-pressed={widgets.toolbarOpen} onclick={onToggleToolbar}>
      {widgets.toolbarOpen ? t(lang, 'toolbar.hide') : t(lang, 'toolbar.open')}
    </button>
    <button
      class="btn-line icon"
      onclick={onRefresh}
      disabled={refreshing}
      aria-label={refreshing ? t(lang, 'catalog.refreshing') : t(lang, 'catalog.refresh')}
      title={refreshing ? t(lang, 'catalog.refreshing') : t(lang, 'catalog.refresh')}
    >
      <span class:spin={refreshing} aria-hidden="true">↻</span>
    </button>
  </header>
  <hr class="plank-rule" />

  {#if launcherUpdate}
    <div class="launcher-update" role="status">
      <div>
        <strong>{t(lang, 'update.available', { v: launcherUpdate.version })}</strong>
        <span class="hint">{t(lang, 'update.note')}</span>
      </div>
      {#if updateProgress === null}
        <button class="btn-gold" onclick={onInstallLauncherUpdate}>{t(lang, 'update.install')}</button>
      {:else}
        <span class="progress">{t(lang, 'update.installing', { p: updateProgress >= 0 ? `${updateProgress} %` : '' })}</span>
      {/if}
    </div>
  {/if}

  {#if widgets.open.length > 0 || widgets.toolbarOpen}
    <!-- Bandeau « en jeu » : une seule ligne, l'état d'abord, les deux bascules ensuite. -->
    <div class="widgets-bar" role="group" aria-label={t(lang, 'widgets.count', { n: String(widgets.open.length) })}>
      <span class="live" class:on={widgets.open.length > 0} title={t(lang, 'widgets.short', { n: String(widgets.open.length) })}>
        <span class="dot" aria-hidden="true"></span><span class="serious">{widgets.open.length}</span>
      </span>
      {#if activeLayout}
        <button class="layout-badge" onclick={() => (tab = 'plugins')} title={t(lang, 'layouts.activeBadge', { name: activeLayout.name })}
          >▦ {activeLayout.name}</button
        >
      {/if}
      <span class="spacer"></span>
      <button
        class="btn-line toggle"
        aria-pressed={widgets.locked}
        title={t(lang, 'widgets.lock')}
        onclick={() => onLockWidgets(!widgets.locked)}
      >
        <span class="bw-ic lock" aria-hidden="true"></span>
        {t(lang, widgets.locked ? 'widgets.locked' : 'widgets.lockShort')}
      </button>
      <button class="btn-line toggle" aria-pressed={widgets.hiddenAll} onclick={onHideWidgets}>
        {t(lang, widgets.hiddenAll ? 'widgets.showAll' : 'widgets.hideAll')}
      </button>
    </div>
  {/if}

  {#if banner}
    <p class="banner" role="status">{banner}</p>
  {/if}

  <div class="tabs" role="tablist" tabindex="-1" onkeydown={onTabKey}>
    {#each tabs as id (id)}
      <button
        id="tab-{id}"
        role="tab"
        aria-selected={tab === id}
        aria-controls="panel"
        tabindex={tab === id ? 0 : -1}
        onclick={() => {
          tab = id;
          category = '';
        }}
      >
        <span class="bw-ic {tabIcon[id]}" aria-hidden="true"></span>
        <span>{t(lang, `tabs.${id}`)}</span>
      </button>
    {/each}
  </div>

  {#if tab !== 'plugins'}
  <div class="filters">
    <span class="search-ic bw-ic search" aria-hidden="true"></span>
    <input
      type="search"
      aria-label={t(lang, 'search.label')}
      placeholder={t(lang, 'search.placeholder')}
      bind:value={query}
    />
    {#if categories.length > 1}
      <select aria-label={t(lang, 'filter.category')} bind:value={category}>
        <option value="">{t(lang, 'filter.all')}</option>
        {#each categories as c (c)}
          <option value={c}>{c}</option>
        {/each}
      </select>
    {/if}
  </div>

  {/if}

  <div id="panel" role="tabpanel" aria-labelledby="tab-{tab}">
    {#if tab === 'plugins'}
      {#if settings && catalog}
        <Plugins
          {lang}
          tools={catalog.tools}
          {isInstalled}
          {icons}
          {widgets}
          {layouts}
          layoutKeys={settings.hotkeys.layouts}
          followDofus={settings.followDofus}
          onLayouts={(v) => (layouts = v)}
          onError={(m) => (error = m)}
        />
      {:else}
        <p class="empty">{t(lang, 'catalog.loading')}</p>
      {/if}
    {:else}
    {#if tab === 'installed' && updatable.length > 0}
      <button class="btn-plank update-all" onclick={onUpdateAll}>
        {t(lang, 'installed.updateAll', { n: String(updatable.length) })}
      </button>
    {/if}
    {#if !catalog}
      <p class="empty">{t(lang, 'catalog.loading')}</p>
    {:else if tab === 'installed' && tabTools.length === 0}
      <div class="empty"><img src={bworkHead} alt="" width="72" height="63" /><p>{t(lang, 'installed.none')}</p></div>
    {:else if visibleTools.length === 0}
      <div class="empty"><img src={bworkHead} alt="" width="72" height="63" /><p>{t(lang, 'catalog.empty')}</p></div>
    {:else}
      <ul class="cards">
        {#each visibleTools as tool (tool.id)}
          <li>
            <ToolCard
              {tool}
              {lang}
              favorite={favorites.includes(tool.id)}
              status={statusOf(tool.id)}
              activity={installs.activity[tool.id]}
              needsUnverifiedConfirm={unverified.includes(tool.id)}
              {onToggleFavorite}
              {onOpenLink}
              {onInstall}
              {onCancel}
              {onUninstall}
              {onLaunch}
              {onDismiss}
              widgetOpen={widgets.open.includes(tool.id)}
              {onWidget}
              icon={icons[tool.id]}
            />
          </li>
        {/each}
      </ul>
    {/if}
    {/if}
  </div>

  {#if error}
    <p class="error" role="alert">{error}</p>
  {/if}

  <footer>
    {#if settings}
      <button class="foot-btn" onclick={() => (showSettings = true)}><span class="bw-ic gears" aria-hidden="true"></span>{t(lang, 'settings.title')}</button>
      <button class="foot-btn" onclick={() => (showShortcuts = true)}><span class="bw-ic pinned" aria-hidden="true"></span>{t(lang, 'shortcuts.open')}</button>
      <button class="foot-btn" onclick={() => (replayTour = true)}><span class="bw-ic idea" aria-hidden="true"></span>{t(lang, 'tour.open')}</button>
    {/if}
    <span class="version">{t(lang, 'footer.version')} {version}</span>
  </footer>
</main>

{#if showTour && settings}
  <Tour {lang} panelKey={settings.hotkeys.togglePanel} lockKey={settings.hotkeys.toggleLock} onDone={onTourDone} />
{/if}

{#if showSettings && settings}
  <SettingsDialog {lang} {settings} onUpdate={update} onClose={() => (showSettings = false)} />
{/if}

{#if showLayouts && settings}
  <LayoutsDialog
    {lang}
    view={layouts}
    keys={settings.hotkeys.layouts}
    followDofus={settings.followDofus}
    onChange={(v) => (layouts = v)}
    onError={(m) => (error = m)}
    onClose={() => (showLayouts = false)}
  />
{/if}

{#if showShortcuts && settings}
  <Shortcuts
    {lang}
    bindings={settings.hotkeys}
    tools={shortcutTools}
    onSave={(hotkeys) => update({ hotkeys })}
    onClose={() => (showShortcuts = false)}
  />
{/if}

<style>
  main {
    display: flex;
    flex-direction: column;
    gap: 12px;
    height: 100vh;
    padding: 14px 16px 12px;
  }
  header {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .logo {
    flex: none;
    filter: drop-shadow(0 2px 2px rgba(0, 0, 0, 0.5));
  }
  .brand {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    line-height: 1;
  }
  .brand h1 {
    font-size: 26px;
    color: var(--text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .brand .kicker {
    font-size: 10.5px;
    letter-spacing: 0.03em;
    margin-top: 3px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  header .btn-gold {
    flex: none;
    padding: 4px 17px;
    font-size: 12.5px;
    white-space: nowrap;
  }
  .btn-line.icon {
    width: 36px;
    height: 32px;
    padding: 0;
    font-size: 20px;
    line-height: 1;
  }
  .spin {
    display: inline-block;
    animation: spin 1s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  .small {
    padding: 1px 10px;
    font-size: 13.5px;
  }
  .widgets-bar {
    display: flex;
    align-items: center;
    gap: var(--s2);
    padding: 5px 6px 5px 10px;
    background: var(--bg-sunken);
    border: 0;
    box-shadow: var(--sunk);
    font-size: 13px;
  }
  .live {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--text-muted);
    white-space: nowrap;
  }
  .live .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: #57523f;
  }
  .live.on {
    color: var(--text);
  }
  .live.on .dot {
    background: var(--green);
    box-shadow: 0 0 0 3px rgba(191, 214, 104, 0.18);
  }
  .spacer {
    flex: 1;
  }
  .toggle {
    --ic: 16px;
    display: inline-flex;
    align-items: center;
    gap: 5px;
    min-height: 26px;
    padding: 0 9px;
    font-size: 12.5px;
    white-space: nowrap;
  }
  .layout-badge {
    min-width: 0;
    max-width: 120px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    border: 0;
    padding: 1px 8px;
    background: #343c22;
    color: #d2dfa5;
    font-size: 12px;
    font-weight: 700;
  }
  .layout-badge:hover {
    background: #46512c;
  }
  .launcher-update {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 10px;
    border: 1px solid var(--accent);
    background: #2f2717;
    font-size: 14px;
  }
  .launcher-update div {
    flex: 1;
    min-width: 0;
  }
  .launcher-update .hint {
    display: block;
    font-size: 12.5px;
    color: var(--text-muted);
  }
  .launcher-update .progress {
    font-size: 13px;
    color: var(--accent);
    white-space: nowrap;
  }
  .banner {
    margin: 0;
    padding: 6px 10px;
    border-left: 4px solid var(--accent);
    background: var(--bg-sunken);
    font-size: 14px;
  }
  .tabs {
    display: flex;
    gap: 4px;
    border-bottom: 1px solid var(--line);
  }
  .tabs button {
    --ic: 26px;
    flex: 1 1 0;
    min-width: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 1px;
    white-space: nowrap;
    padding: 2px 2px 5px;
    border: 0;
    border-bottom: 3px solid transparent;
    margin-bottom: -1px;
    background: transparent;
    color: var(--text-muted);
    font-family: var(--font-paint);
    font-size: 13.5px;
    letter-spacing: 0;
    transition: color 0.12s, border-color 0.12s;
  }
  .tabs .bw-ic {
    filter: grayscale(0.6) brightness(0.75);
    transition: filter 0.12s, transform 0.12s;
  }
  .tabs button:hover .bw-ic {
    filter: none;
  }
  .tabs button[aria-selected='true'] .bw-ic {
    filter: drop-shadow(0 2px 1px #000a);
    transform: translateY(-1px) rotate(-4deg);
  }
  .tabs button:hover {
    color: var(--text);
  }
  .tabs button[aria-selected='true'] {
    color: var(--accent);
    border-bottom-color: var(--accent);
  }
  .filters {
    position: relative;
    display: flex;
    gap: 8px;
  }
  .search-ic {
    --ic: 22px;
    position: absolute;
    left: 6px;
    top: 50%;
    transform: translateY(-50%);
    pointer-events: none;
  }
  .filters input[type='search'] {
    padding-left: 34px;
  }
  .filters input {
    flex: 1;
    min-width: 0;
  }
  input[type='search'],
  select {
    min-height: 34px;
    padding: 2px 10px;
    background: var(--bg-sunken);
    color: var(--text);
    border: 0;
    box-shadow: var(--sunk);
    font: inherit;
    font-family: var(--font-serious);
    font-size: 14px;
    letter-spacing: 0;
  }
  input[type='search']:focus,
  select:focus {
    border-color: var(--accent);
  }
  input[type='search']::placeholder {
    color: var(--text-muted);
  }
  #panel {
    flex: 1;
    overflow-y: auto;
    overflow-x: hidden;
    /* Place pour les équerres qui dépassent des cartes. */
    padding: 6px 6px 8px;
    margin: 0 -4px;
  }
  .cards {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .empty {
    color: var(--text-muted);
    text-align: center;
    margin-top: 24px;
  }
  div.empty img {
    display: block;
    margin: 0 auto 6px;
    filter: drop-shadow(0 4px 3px #0006);
  }
  div.empty p {
    margin: 0;
    font-size: 16px;
  }
  /* Pied de page : liens à icône dessinée, sans cadre. */
  .foot-btn {
    --ic: 22px;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    border: 0;
    background: transparent;
    padding: 2px 4px;
    color: var(--text-muted);
    font-family: var(--font-paint);
    font-size: 13.5px;
  }
  .foot-btn:hover {
    color: var(--accent);
  }
  .foot-btn:hover .bw-ic {
    transform: rotate(-6deg);
  }
  .foot-btn .bw-ic {
    transition: transform 0.12s;
  }
  .update-all {
    width: 100%;
    margin-bottom: 14px;
  }
  .error {
    color: var(--danger);
    margin: 0;
  }
  footer {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    padding-top: 8px;
    border-top: 1px solid var(--line);
  }
  .version {
    margin-left: auto;
    font-size: 13px;
    color: var(--text-muted);
  }
  @media (prefers-reduced-motion: reduce) {
    .spin {
      animation: none;
    }
  }
</style>
