<script lang="ts">
  // Page « Raccourcis » : voir, changer, désactiver les raccourcis globaux
  // (actions du launcher, puis un raccourci par outil pour l'ouvrir ou le fermer).
  // Pendant la saisie, les raccourcis du launcher sont suspendus (sinon Windows les garderait pour lui).
  import { onMount } from 'svelte';
  import {
    getHotkeysStatus,
    onHotkeysStatus,
    suspendHotkeys,
    type HotkeyAction,
    type HotkeyBindings,
    type HotkeyStatus,
    type Lang,
  } from '../shared/ipc';
  import { t } from '../shared/i18n';

  interface Props {
    lang: Lang;
    bindings: HotkeyBindings;
    /** Outils qui peuvent avoir un raccourci (installés, ou avec une version web). */
    tools: { id: string; name: string }[];
    onSave: (bindings: HotkeyBindings) => Promise<void>;
    onClose: () => void;
  }
  let { lang, bindings, tools, onSave, onClose }: Props = $props();

  const ACTIONS: HotkeyAction[] = ['togglePanel', 'toggleHideAll', 'toggleLock'];
  const DEFAULTS: Record<HotkeyAction, string> = { togglePanel: 'Alt+B', toggleHideAll: 'Alt+H', toggleLock: 'Alt+L' };

  /** Une ligne : clé d'état (« togglePanel » ou « tool:<id> »), libellé, valeur, valeur par défaut. */
  interface Row {
    key: string;
    label: string;
    value: string | null;
    def: string | null;
  }

  const actionRows = $derived<Row[]>(
    ACTIONS.map((a) => ({ key: a, label: t(lang, `shortcuts.${a}`), value: bindings[a], def: DEFAULTS[a] })),
  );
  // Dispositions 1 à 4 (Alt+1 à Alt+4 par défaut).
  const layoutRows = $derived<Row[]>(
    [0, 1, 2, 3].map((i) => ({
      key: `layout:${i + 1}`,
      label: t(lang, 'shortcuts.layout', { n: String(i + 1) }),
      value: bindings.layouts[i] ?? null,
      def: `Alt+${i + 1}`,
    })),
  );
  const toolRows = $derived<Row[]>(
    tools.map((x) => ({ key: `tool:${x.id}`, label: x.name, value: bindings.widgets[x.id] ?? null, def: null })),
  );

  let status = $state<HotkeyStatus[]>([]);
  let capturing = $state<string | null>(null);
  let dialog: HTMLDivElement | undefined = $state();

  const statusOf = (key: string) => status.find((s) => s.action === key);

  onMount(() => {
    getHotkeysStatus().then((s) => (status = s));
    const off = onHotkeysStatus((s) => (status = s));
    dialog?.querySelector<HTMLButtonElement>('button')?.focus();
    return () => {
      off.then((f) => f());
      if (capturing) suspendHotkeys(false);
    };
  });

  // Touche du clavier (KeyboardEvent.code) → nom compris par le launcher.
  function keyName(code: string): string | null {
    let m: RegExpMatchArray | null;
    if ((m = code.match(/^Key([A-Z])$/))) return m[1];
    if ((m = code.match(/^Digit(\d)$/))) return m[1];
    if ((m = code.match(/^Numpad(\d)$/))) return `Num${m[1]}`;
    if (/^F([1-9]|1\d|2[0-4])$/.test(code)) return code;
    const named: Record<string, string> = {
      Space: 'Space',
      PageUp: 'PageUp',
      PageDown: 'PageDown',
      Home: 'Home',
      End: 'End',
      Insert: 'Insert',
      Delete: 'Delete',
      ArrowUp: 'Up',
      ArrowDown: 'Down',
      ArrowLeft: 'Left',
      ArrowRight: 'Right',
    };
    return named[code] ?? null;
  }

  async function startCapture(key: string) {
    capturing = key;
    await suspendHotkeys(true);
  }

  async function stopCapture() {
    capturing = null;
    await suspendHotkeys(false);
  }

  async function set(key: string, value: string | null) {
    if (key.startsWith('layout:')) {
      const i = Number(key.slice(7)) - 1;
      const layouts = [...bindings.layouts];
      while (layouts.length < 4) layouts.push(null);
      layouts[i] = value;
      await onSave({ ...bindings, layouts });
    } else if (key.startsWith('tool:')) {
      const id = key.slice(5);
      const widgets = { ...bindings.widgets };
      if (value) widgets[id] = value;
      else delete widgets[id];
      await onSave({ ...bindings, widgets });
    } else {
      await onSave({ ...bindings, [key]: value });
    }
  }

  async function onKey(e: KeyboardEvent) {
    if (!capturing) {
      if (e.key === 'Escape') onClose();
      return;
    }
    e.preventDefault();
    e.stopPropagation();
    if (e.key === 'Escape') {
      await stopCapture();
      return;
    }
    const key = keyName(e.code);
    if (!key) return; // modificateur seul, ou touche non prise en charge : on attend la suite
    const mods = [e.ctrlKey && 'Ctrl', e.altKey && 'Alt', e.shiftKey && 'Shift', e.metaKey && 'Win'].filter(Boolean);
    const target = capturing;
    capturing = null;
    await set(target, [...mods, key].join('+'));
    await suspendHotkeys(false);
  }
</script>

<svelte:window onkeydown={onKey} />

{#snippet line(row: Row)}
  {@const s = statusOf(row.key)}
  <li>
    <div class="row">
      <span class="label">{row.label}</span>
      {#if capturing === row.key}
        <kbd class="capture" aria-live="polite">{t(lang, 'shortcuts.press')}</kbd>
      {:else}
        <kbd class:off={!row.value}>{row.value ? (s?.hotkey ?? row.value) : t(lang, 'shortcuts.none')}</kbd>
      {/if}
    </div>
    <div class="row actions">
      <span class="state" class:bad={!!s?.error} class:ok={s?.active}>
        {#if s?.error}{t(lang, 'shortcuts.error', { reason: s.error })}
        {:else if s?.active}{t(lang, 'shortcuts.active')}
        {:else if !row.value}{t(lang, 'shortcuts.off')}
        {/if}
      </span>
      {#if capturing === row.key}
        <button onclick={stopCapture}>{t(lang, 'shortcuts.cancel')}</button>
      {:else}
        <button onclick={() => startCapture(row.key)} disabled={capturing !== null}>
          {t(lang, row.value ? 'shortcuts.change' : 'shortcuts.set')}
        </button>
        {#if row.value}
          <button onclick={() => set(row.key, null)} disabled={capturing !== null}>{t(lang, 'shortcuts.disable')}</button>
        {/if}
        {#if row.def && row.value !== row.def}
          <button onclick={() => set(row.key, row.def)} disabled={capturing !== null}>{t(lang, 'shortcuts.default')}</button>
        {/if}
      {/if}
    </div>
  </li>
{/snippet}

<div class="backdrop">
  <div class="dialog frame" role="dialog" aria-modal="true" aria-labelledby="shortcuts-title" bind:this={dialog}>
    <header class="dialog-head">
      <h2 id="shortcuts-title">{t(lang, 'shortcuts.title')}</h2>
      <button class="dialog-close" onclick={onClose} aria-label={t(lang, 'shortcuts.close')} title={t(lang, 'shortcuts.close')}>×</button>
    </header>
    <p class="note serious">{t(lang, 'shortcuts.note')}</p>

    <ul>
      {#each actionRows as row (row.key)}{@render line(row)}{/each}
      {#each layoutRows as row (row.key)}{@render line(row)}{/each}
    </ul>

    {#if toolRows.length > 0}
      <h3>{t(lang, 'shortcuts.tools')}</h3>
      <p class="note serious">{t(lang, 'shortcuts.toolsNote')}</p>
      <ul>
        {#each toolRows as row (row.key)}{@render line(row)}{/each}
      </ul>
    {/if}
  </div>
</div>

<style>
  h3 {
    margin: 16px 0 0;
    font-size: 20px;
    color: var(--accent);
  }
  .note {
    margin: 4px 0 10px;
    font-size: 13.5px;
    color: var(--text-muted);
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  li {
    padding: 8px 10px;
    border: 0;
    box-shadow: var(--sunk);
    background: var(--bg-sunken);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .label {
    flex: 1;
    font-size: 15px;
    font-weight: 600;
  }
  kbd {
    font-family: inherit;
    font-weight: 700;
    padding: 2px 9px;
    border: 1px solid var(--wood-light);
    border-bottom-width: 3px;
    border-radius: 2px;
    background: #2f2a20;
    color: var(--text);
    font-size: 13.5px;
  }
  kbd.off {
    font-weight: 400;
    color: var(--text-muted);
    border-color: var(--line);
  }
  kbd.capture {
    border-color: var(--accent);
    color: var(--accent);
  }
  .actions {
    margin-top: 6px;
    flex-wrap: wrap;
  }
  .state {
    flex: 1;
    font-size: 13px;
    color: var(--text-muted);
  }
  .state.ok {
    color: var(--green);
  }
  .state.bad {
    color: var(--danger);
  }
  .actions button {
    border: 0;
    background:
      repeating-linear-gradient(0deg, #0000 0 4px, #00000026 4px 5px, #0000 5px 9px, #ffffff0a 9px 10px),
      linear-gradient(180deg, #6b4526, #4f321b 60%, #3f2714);
    box-shadow: inset 0 1px 0 #a0703f80, inset 0 -1px 0 #1d1209, 0 2px 0 #0c0e0a;
    color: #f6e6c6;
    padding: 1px 9px;
    font-size: 13px;
  }
  .actions button:hover:not(:disabled) {
    filter: brightness(1.18);
  }
</style>
