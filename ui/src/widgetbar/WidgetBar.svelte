<script lang="ts">
  // Barre d'un widget : déplacer, replier en pastille, recharger, réglages, fermer.
  // Les commandes n'agissent que sur le widget de cette barre (côté Rust, par l'étiquette de la vue).
  import { onMount } from 'svelte';
  import {
    onWidgetBarState,
    widgetBarState,
    widgetClose,
    widgetDrag,
    widgetReload,
    widgetSetOpacity,
    widgetSettings,
    widgetToggleCollapse,
    type BarState,
  } from '../shared/ipc';
  import { t } from '../shared/i18n';

  let bar = $state<BarState | null>(null);
  let opacity = $state(100);
  const lang = $derived(bar?.language ?? 'fr');
  const initial = $derived((bar?.name ?? '?').trim().charAt(0).toUpperCase());

  // Pas de zone de message dans cette petite barre : un échec est noté dans la console.
  const run = (p: Promise<unknown>) => p.catch((e) => console.warn(e));

  function apply(s: BarState) {
    bar = s;
    opacity = Math.round(s.opacity * 100);
    document.documentElement.lang = s.language;
  }

  onMount(() => {
    run(widgetBarState().then((s) => s && apply(s)));
    const off = onWidgetBarState((s) => {
      if (!bar || s.id === bar.id) apply(s);
    });
    return () => {
      off.then((f) => f());
    };
  });

  // Bouton gauche sur la zone de titre : déplacement ; double clic : replier / déplier.
  // Le double clic est compté ici (deux appuis rapprochés) : sur `pointerdown`, `detail`
  // vaut toujours 0, et le déplacement lancé au premier appui avale le `dblclick`.
  const DOUBLE_MS = 500;
  let lastDown = { at: -Infinity, x: 0, y: 0 };
  function onPointerDown(e: PointerEvent) {
    if (e.button !== 0) return;
    // Même endroit de l'écran : un widget déplacé entre les deux appuis n'est pas replié.
    const near = Math.abs(e.screenX - lastDown.x) <= 4 && Math.abs(e.screenY - lastDown.y) <= 4;
    if (e.timeStamp - lastDown.at < DOUBLE_MS && near) {
      lastDown = { at: -Infinity, x: 0, y: 0 };
      run(widgetToggleCollapse());
      return;
    }
    lastDown = { at: e.timeStamp, x: e.screenX, y: e.screenY };
    run(widgetDrag());
  }

  let pending: ReturnType<typeof setTimeout> | undefined;
  function onOpacityInput() {
    // Appliqué tout de suite, sans attendre le relâchement du curseur.
    clearTimeout(pending);
    pending = setTimeout(() => run(widgetSetOpacity(opacity / 100)), 16);
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape' && bar?.settingsOpen) run(widgetSettings(false));
  }
</script>

<svelte:window onkeydown={onKey} />

{#if bar}
  <div class="bar" class:pill={bar.collapsed}>
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="title" onpointerdown={onPointerDown} title={t(lang, 'widget.dragHint')}>
      <span class="medallion" aria-hidden="true">{initial}</span>
      <span class="name">{bar.name}</span>
      {#if bar.locked}<span class="lock" title={t(lang, 'widget.locked')}>🔒</span>{/if}
    </div>
    <div class="buttons">
      <button
        type="button"
        aria-label={bar.collapsed ? t(lang, 'widget.expand') : t(lang, 'widget.collapse')}
        title={bar.collapsed ? t(lang, 'widget.expand') : t(lang, 'widget.collapse')}
        onclick={() => run(widgetToggleCollapse())}
      >
        {#if bar.collapsed}
          <svg viewBox="0 0 16 16" aria-hidden="true"><rect x="3" y="3" width="10" height="10" rx="1.5" /></svg>
        {:else}
          <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3 12h10" /></svg>
        {/if}
      </button>
      {#if !bar.collapsed}
        <button type="button" aria-label={t(lang, 'widget.reload')} title={t(lang, 'widget.reload')} onclick={() => run(widgetReload())}>
          <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M13 8a5 5 0 1 1-1.5-3.5M13 2.5v3h-3" /></svg>
        </button>
        <button
          type="button"
          aria-label={t(lang, 'widget.settings')}
          title={t(lang, 'widget.settings')}
          aria-expanded={bar.settingsOpen}
          class:active={bar.settingsOpen}
          onclick={() => run(widgetSettings(!bar?.settingsOpen))}
        >
          <svg viewBox="0 0 16 16" aria-hidden="true"><circle cx="8" cy="8" r="2.2" /><path d="M8 1.5v2M8 12.5v2M1.5 8h2M12.5 8h2M3.4 3.4l1.4 1.4M11.2 11.2l1.4 1.4M3.4 12.6l1.4-1.4M11.2 4.8l1.4-1.4" /></svg>
        </button>
      {/if}
      <button type="button" class="close" aria-label={t(lang, 'widget.close')} title={t(lang, 'widget.close')} onclick={() => run(widgetClose())}>
        <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M4 4l8 8M12 4l-8 8" /></svg>
      </button>
    </div>
  </div>

  {#if bar.settingsOpen && !bar.collapsed}
    <div class="settings" role="dialog" aria-label={t(lang, 'widget.settings')}>
      <label for="opacity">{t(lang, 'widget.opacity')} <strong>{opacity} %</strong></label>
      <input id="opacity" type="range" min="30" max="100" step="5" bind:value={opacity} oninput={onOpacityInput} />
      <p class="hint serious">{t(lang, 'widget.lockHint')}</p>
    </div>
  {/if}
{/if}

<style>
  :global(body.widgetbar) {
    overflow: hidden;
    user-select: none;
    font-size: 13px;
    background: var(--bg);
  }

  .bar {
    display: flex;
    align-items: center;
    height: 24px;
    background: linear-gradient(#a8743d, #7a4f27 55%, #5c3a1c);
    border-bottom: 1px solid #3b2714;
  }

  .bar.pill {
    border-radius: 0;
    border: 1px solid var(--wood-light);
    height: 24px;
  }

  .title {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 6px;
    padding-left: 4px;
    height: 100%;
    cursor: grab;
  }

  .medallion {
    flex: none;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    background: var(--parchment);
    color: var(--ink);
    font-family: var(--font-title);
    font-weight: 700;
    font-size: 13px;
    line-height: 1;
  }

  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--font-paint);
    font-size: 13.5px;
    color: #fff1d4;
    text-shadow: 0 1px 0 #2a1a0d;
  }

  .lock {
    font-size: 11px;
  }

  .buttons {
    display: flex;
    flex: none;
    height: 100%;
  }

  button {
    width: 26px;
    height: 24px;
    min-height: 24px;
    padding: 0;
    border: 0;
    background: transparent;
    display: grid;
    place-items: center;
    color: var(--text);
  }

  button:hover,
  button.active {
    background: rgba(0, 0, 0, 0.25);
  }

  button.close:hover {
    background: #8c2f22;
  }

  button:focus-visible {
    outline-offset: -2px;
  }

  svg {
    width: 14px;
    height: 14px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.8;
    stroke-linecap: round;
    stroke-linejoin: round;
  }

  .settings {
    padding: 8px 10px;
    background: var(--bg-raised);
    border-bottom: 2px solid var(--wood);
    display: grid;
    gap: 6px;
  }

  .settings input[type='range'] {
    width: 100%;
    min-height: 24px;
    accent-color: var(--accent);
  }

  .hint {
    margin: 0;
    color: var(--text-muted);
    font-size: 12px;
  }
</style>
