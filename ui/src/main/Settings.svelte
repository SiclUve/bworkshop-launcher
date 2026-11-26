<script lang="ts">
  // Fenêtre « Réglages » : langue, démarrage avec Windows, suivi de Dofus.
  // Chaque case s'enregistre aussitôt cochée.
  import { onMount } from 'svelte';
  import type { Lang, Settings } from '../shared/ipc';
  import { t } from '../shared/i18n';
  import { trapFocus } from '../shared/focus';

  interface Props {
    lang: Lang;
    settings: Settings;
    onUpdate: (patch: Partial<Settings>) => Promise<void>;
    onClose: () => void;
  }
  let { lang, settings, onUpdate, onClose }: Props = $props();

  let dialog: HTMLDivElement | undefined = $state();
  onMount(() => dialog?.querySelector<HTMLElement>('select')?.focus());

  // Enregistre, puis remet le champ sur la valeur réellement enregistrée (après un échec,
  // ou une valeur corrigée par le launcher, comme un titre vide remis à « Dofus »).
  async function save<K extends keyof Settings>(el: HTMLInputElement | HTMLSelectElement, key: K, value: Settings[K]) {
    await onUpdate({ [key]: value } as Partial<Settings>);
    const saved = settings[key];
    if (el instanceof HTMLInputElement && el.type === 'checkbox') el.checked = Boolean(saved);
    else el.value = String(saved);
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape') onClose();
  }
</script>

<svelte:window onkeydown={onKey} />

{#snippet toggle(key: 'autostart' | 'followDofus' | 'snapping' | 'onlyWhenDofusForeground' | 'reopenLastLayout', label: string, hint: string)}
  <label class="option">
    <input type="checkbox" checked={settings[key]} onchange={(e) => save(e.currentTarget, key, e.currentTarget.checked)} />
    <span>
      <span class="name">{label}</span>
      {#if hint}<span class="hint serious">{hint}</span>{/if}
    </span>
  </label>
{/snippet}

<div class="backdrop">
  <div class="dialog frame" role="dialog" aria-modal="true" aria-labelledby="settings-title" bind:this={dialog} use:trapFocus>
    <header class="dialog-head">
      <h2 id="settings-title">{t(lang, 'settings.title')}</h2>
      <button class="dialog-close" onclick={onClose} aria-label={t(lang, 'shortcuts.close')} title={t(lang, 'shortcuts.close')}>×</button>
    </header>

    <section>
      <h3>{t(lang, 'settings.general')}</h3>
      <label class="option lang">
        <span class="name">{t(lang, 'settings.language')}</span>
        <select value={settings.language} onchange={(e) => save(e.currentTarget, 'language', e.currentTarget.value as Lang)}>
          <option value="fr">Français</option>
          <option value="en">English</option>
        </select>
      </label>
      {@render toggle('autostart', t(lang, 'settings.autostart'), '')}
      {@render toggle('snapping', t(lang, 'settings.snapping'), t(lang, 'settings.snappingHint'))}
      {@render toggle('reopenLastLayout', t(lang, 'settings.reopenLayout'), '')}
    </section>

    <section>
      <h3>{t(lang, 'settings.game')}</h3>
      {@render toggle('followDofus', t(lang, 'settings.followDofus'), t(lang, 'settings.followDofusHint'))}
      <div class="sub" class:off={!settings.followDofus}>
        {#if settings.followDofus}
          {@render toggle('onlyWhenDofusForeground', t(lang, 'settings.onlyForeground'), t(lang, 'settings.onlyForegroundHint'))}
          <label class="option pattern">
            <span>
              <span class="name">{t(lang, 'settings.pattern')}</span>
              <span class="hint serious">{t(lang, 'settings.patternHint')}</span>
            </span>
            <input
              type="text"
              value={settings.dofusTitlePattern}
              maxlength="60"
              spellcheck="false"
              onchange={(e) => save(e.currentTarget, 'dofusTitlePattern', e.currentTarget.value)}
            />
          </label>
        {/if}
      </div>
    </section>
  </div>
</div>

<style>
  section {
    margin-top: 14px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  h3 {
    font-size: 17px;
    color: var(--accent);
    margin-bottom: 2px;
  }
  .option {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 8px 10px;
    border: 0;
    box-shadow: var(--sunk);
    background: var(--bg-sunken);
    cursor: pointer;
  }
  .option:hover {
    background: #171b13;
  }
  .option.lang {
    align-items: center;
    justify-content: space-between;
  }
  .name {
    display: block;
    font-weight: 700;
    font-size: 15px;
  }
  .hint {
    display: block;
    margin-top: 2px;
    font-size: 13.5px;
    color: var(--text-muted);
    line-height: 1.4;
  }
  .sub {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-left: 22px;
    padding-left: 10px;
    border-left: 3px solid var(--wood);
  }
  .sub.off {
    display: none;
  }
  input[type='checkbox'] {
    flex: none;
    width: 20px;
    height: 20px;
    margin: 1px 0 0;
    accent-color: var(--accent);
  }
  .option.pattern {
    flex-direction: column;
    gap: 6px;
    cursor: default;
  }
  input[type='text'] {
    width: 100%;
    min-height: 30px;
    padding: 2px 8px;
    background: var(--bg-raised);
    color: var(--text);
    border: 0;
    box-shadow: var(--sunk);
    font: inherit;
  }
  input[type='text']:focus {
    border-color: var(--accent);
  }
  select {
    min-height: 30px;
    padding: 2px 8px;
    background: var(--bg-raised);
    color: var(--text);
    border: 0;
    box-shadow: var(--sunk);
    font: inherit;
  }
</style>
