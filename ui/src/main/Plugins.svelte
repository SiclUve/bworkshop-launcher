<script lang="ts">
  // Onglet « Mes plugins » : pour chaque outil installé ou disponible en version web,
  // une tuile dans la barre ou non, et ce qu'elle ouvre (application, overlay web, page web).
  // En dessous : les dispositions, modifiables sur place.
  import { onMount } from 'svelte';
  import {
    availableModes,
    effectiveMode,
    getToolbarPrefs,
    inBar,
    launchTool,
    onToolbarPrefs,
    setToolbarOpen,
    setToolInBar,
    setToolMode,
    toggleTool,
    type Lang,
    type LayoutsView,
    type OpenMode,
    type Tool,
    type ToolbarPrefs,
    type WidgetsView,
  } from '../shared/ipc';
  import { t } from '../shared/i18n';
  import LayoutsPanel from './Layouts.svelte';

  interface Props {
    lang: Lang;
    tools: Tool[];
    isInstalled: (id: string) => boolean;
    icons: Record<string, string>;
    widgets: WidgetsView;
    layouts: LayoutsView;
    layoutKeys: (string | null)[];
    followDofus: boolean;
    onLayouts: (view: LayoutsView) => void;
    onError: (message: string) => void;
  }
  let { lang, tools, isInstalled, icons, widgets, layouts, layoutKeys, followDofus, onLayouts, onError }: Props = $props();

  let prefs = $state<ToolbarPrefs>({ hidden: [], shown: [], modes: {}, order: [], vertical: false, tile: 44 });

  onMount(() => {
    getToolbarPrefs()
      .then((p) => (prefs = p))
      .catch((e) => onError(String(e)));
    const off = onToolbarPrefs((p) => (prefs = p));
    return () => {
      off.then((f) => f());
    };
  });

  // Installés d'abord, puis par nom ; seulement ce qui peut s'ouvrir d'une façon ou d'une autre.
  const rows = $derived(
    tools
      .map((tool) => ({ tool, installed: isInstalled(tool.id), modes: availableModes(tool, isInstalled(tool.id)) }))
      .filter((r) => r.modes.length > 0)
      .sort((a, b) => Number(b.installed) - Number(a.installed) || a.tool.order - b.tool.order || a.tool.name.localeCompare(b.tool.name)),
  );
  const nameOf = (id: string) => tools.find((x) => x.id === id)?.name ?? id;

  async function run(op: () => Promise<unknown>) {
    try {
      await op();
    } catch (e) {
      onError(String(e));
    }
  }

  function chooseMode(tool: Tool, mode: OpenMode) {
    prefs = { ...prefs, modes: { ...prefs.modes, [tool.id]: mode } };
    run(() => setToolMode(tool.id, mode));
  }

  function setBar(id: string, value: boolean) {
    run(() => setToolInBar(id, value));
  }

  function open(tool: Tool, installed: boolean) {
    const mode = effectiveMode(prefs, tool, installed);
    run(() => (mode === 'app' ? launchTool(tool.id) : toggleTool(tool.id)));
  }
</script>

<section class="plugins" aria-labelledby="plugins-title">
  <h2 id="plugins-title" class="visually-hidden">{t(lang, 'tabs.plugins')}</h2>
  <p class="intro serious">{t(lang, 'plugins.intro')}</p>

  {#if !widgets.toolbarOpen}
    <p class="bar-closed" role="status">
      {t(lang, 'plugins.toolbarClosed')}
      <button class="btn-line small" onclick={() => run(() => setToolbarOpen(true))}>{t(lang, 'plugins.openToolbar')}</button>
    </p>
  {/if}

  {#if rows.length === 0}
    <p class="empty">{t(lang, 'plugins.none')}</p>
  {:else}
    <div class="cols" aria-hidden="true">
      <span>{t(lang, 'plugins.barCol')}</span><span>{t(lang, 'plugins.toolCol')}</span>
    </div>
    <ul class="rows">
      {#each rows as { tool, installed, modes } (tool.id)}
        {@const mode = effectiveMode(prefs, tool, installed)}
        {@const webOpen = widgets.open.includes(tool.id)}
        <li class="row" class:off={!inBar(prefs, tool.id, installed)}>
          <div class="line">
          <label class="check">
            <input type="checkbox" checked={inBar(prefs, tool.id, installed)} onchange={(e) => setBar(tool.id, e.currentTarget.checked)} />
            <span class="visually-hidden">{t(lang, 'plugins.inBar')} : {tool.name}</span>
          </label>
          {#if icons[tool.id]}
            <img class="icon" src={icons[tool.id]} alt="" />
          {:else}
            <span class="icon initial" aria-hidden="true">{tool.name.trim().charAt(0).toUpperCase()}</span>
          {/if}
          <span class="name">
            <span class="title">{tool.name}</span>
            {#if !installed}<small>{t(lang, 'plugins.notInstalled')}</small>{/if}
          </span>
          <button class="btn-line small open" onclick={() => open(tool, installed)}>
            {mode !== 'app' && webOpen ? t(lang, 'plugins.close') : t(lang, 'plugins.open')}
          </button>
          </div>
          {#if modes.length === 1}
            <p class="only">{t(lang, 'plugins.only', { mode: t(lang, `mode.${modes[0]}`).toLowerCase() })}</p>
          {:else}
          <div class="modes" role="radiogroup" aria-label="{t(lang, 'plugins.mode')} : {tool.name}">
            {#each modes as m (m)}
              <button
                type="button"
                role="radio"
                aria-checked={mode === m}
                class:on={mode === m}
                title={t(lang, `mode.${m}Hint`)}
                onclick={() => chooseMode(tool, m)}>{t(lang, `mode.${m}`)}</button
              >
            {/each}
          </div>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}

  <div class="layouts">
    <LayoutsPanel
      inline
      {lang}
      view={layouts}
      keys={layoutKeys}
      {followDofus}
      toolName={nameOf}
      onChange={onLayouts}
      {onError}
      onClose={() => {}}
    />
  </div>
</section>

<style>
  .intro {
    margin: 0 0 10px;
    color: var(--text-muted);
    font-size: 13.5px;
  }
  .bar-closed {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 0 0 10px;
    padding: 6px 10px;
    border: 0;
    box-shadow: var(--sunk);
    font-size: 13.5px;
  }
  .empty {
    color: var(--text-muted);
    text-align: center;
    margin: 18px 0;
  }
  .cols {
    display: grid;
    grid-template-columns: 56px 1fr;
    padding: 0 11px 4px;
    font-size: 11px;
    font-weight: 800;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--text-muted);
  }
  .only {
    margin: 0 0 0 66px;
    font-size: 12.5px;
    color: var(--text-muted);
  }
  .rows {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .row {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 8px 10px;
    border: 0;
    box-shadow: var(--sunk);
    background: var(--bg-sunken);
    transition: opacity 0.15s;
  }
  /* Hors de la barre : ligne atténuée (le mode reste réglable). */
  .row.off {
    opacity: 0.6;
  }
  .line {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .check {
    display: flex;
  }
  .check input {
    width: 18px;
    height: 18px;
    accent-color: var(--accent);
    cursor: pointer;
  }
  .icon {
    width: 28px;
    height: 28px;
    flex: none;
    object-fit: contain;
  }
  .initial {
    display: grid;
    place-items: center;
    font-weight: 700;
    background: var(--bg);
    border: 1px solid var(--line);
  }
  .name {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    line-height: 1.2;
  }
  .title {
    font-weight: 700;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .name small {
    font-weight: 400;
    font-size: 11.5px;
    color: var(--text-muted);
  }
  .modes {
    display: flex;
    margin-left: 66px; /* aligné sous le nom (case + icône) */
    gap: 3px;
  }
  .modes button {
    flex: 1;
    box-shadow: var(--sunk);
    padding: 3px 6px;
    font: inherit;
    font-size: 12.5px;
    background: var(--bg-sunken);
    color: var(--text-muted);
    border: 0;
    cursor: pointer;
  }
  .modes button:hover {
    color: var(--text);
  }
  .modes button.on {
    background: var(--accent);
    color: var(--bg);
    font-weight: 700;
  }
  .open {
    flex: none;
    min-width: 70px;
  }
  .layouts {
    margin-top: 22px;
    padding-top: 14px;
    border-top: 1px solid var(--line);
  }
</style>
