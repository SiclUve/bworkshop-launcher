<script lang="ts">
  // « Dispositions » : enregistrer l'état actuel, rappeler, renommer, réordonner, supprimer,
  // et retirer un outil (ou la barre) d'une disposition. En fenêtre, ou dans l'onglet « Mes plugins ».
  import { onMount } from 'svelte';
  import {
    layoutApply,
    layoutDelete,
    layoutMove,
    layoutOverwrite,
    layoutRename,
    layoutSave,
    layoutRemoveItem,
    LAYOUT_TOOLBAR,
    type Lang,
    type LayoutsView,
  } from '../shared/ipc';
  import { t } from '../shared/i18n';

  interface Props {
    lang: Lang;
    view: LayoutsView;
    /** Raccourcis des dispositions 1 à 4, tels que réglés. */
    keys: (string | null)[];
    followDofus: boolean;
    onChange: (view: LayoutsView) => void;
    onError: (message: string) => void;
    onClose: () => void;
    /** Nom affiché d'un outil. */
    toolName?: (id: string) => string;
    /** Dans une page (onglet) plutôt qu'en fenêtre par-dessus. */
    inline?: boolean;
  }
  let { lang, view, keys, followDofus, onChange, onError, onClose, toolName = (id) => id, inline = false }: Props = $props();

  let name = $state('');
  let renaming = $state<number | null>(null);
  let renameText = $state('');
  let confirmDelete = $state<number | null>(null);
  let dialog: HTMLElement | undefined = $state();

  onMount(() => {
    if (!inline) dialog?.querySelector<HTMLInputElement>('input')?.focus();
  });

  const keysText = $derived(
    keys
      .slice(0, 4)
      .map((k) => k ?? '—')
      .join(', '),
  );

  async function run(op: () => Promise<LayoutsView | void>) {
    try {
      const v = await op();
      if (v) onChange(v);
    } catch (e) {
      onError(String(e));
    }
  }

  async function save() {
    await run(() => layoutSave(name));
    name = '';
  }

  async function finishRename(i: number) {
    renaming = null;
    await run(() => layoutRename(i, renameText));
  }

  function onKey(e: KeyboardEvent) {
    if (e.key !== 'Escape') return;
    if (renaming !== null) renaming = null;
    else if (confirmDelete !== null) confirmDelete = null;
    else if (!inline) onClose();
  }
</script>

<svelte:window onkeydown={onKey} />

{#if inline}
  <section class="inline" aria-labelledby="layouts-title" bind:this={dialog}>
    <h2 id="layouts-title">{t(lang, 'layouts.title')}</h2>
    {@render body()}
  </section>
{:else}
  <div class="backdrop">
    <div class="dialog frame" role="dialog" aria-modal="true" aria-labelledby="layouts-title" bind:this={dialog}>
      <header class="dialog-head">
        <h2 id="layouts-title">{t(lang, 'layouts.title')}</h2>
        <button class="dialog-close" onclick={onClose} aria-label={t(lang, 'shortcuts.close')} title={t(lang, 'shortcuts.close')}>×</button>
      </header>
      {@render body()}
    </div>
  </div>
{/if}

{#snippet body()}
    <p class="note serious">{t(lang, 'layouts.note', { keys: keysText })}</p>

    <form
      class="new"
      onsubmit={(e) => {
        e.preventDefault();
        save();
      }}
    >
      <input type="text" maxlength="40" bind:value={name} aria-label={t(lang, 'layouts.newName')} placeholder={t(lang, 'layouts.newPlaceholder')} />
      <button class="btn-plank" type="submit">{t(lang, 'layouts.save')}</button>
    </form>

    {#if view.layouts.length === 0}
      <p class="empty">{t(lang, 'layouts.empty')}</p>
    {:else}
      <ol>
        {#each view.layouts as l, i (i)}
          <li class:active={view.active === i}>
            <div class="row">
              <span class="num" aria-hidden="true">{i + 1}</span>
              {#if renaming === i}
                <input
                  class="rename"
                  type="text"
                  maxlength="40"
                  bind:value={renameText}
                  aria-label={t(lang, 'layouts.rename')}
                  onkeydown={(e) => e.key === 'Enter' && finishRename(i)}
                  onblur={() => finishRename(i)}
                />
              {:else}
                <span class="name">{l.name}</span>
              {/if}
              {#if i < 4 && keys[i]}<kbd>{keys[i]}</kbd>{/if}
              <button class="btn-line small" onclick={() => run(() => layoutApply(i))}>{t(lang, 'layouts.apply')}</button>
            </div>
            <div class="row meta">
              <span>
                {t(lang, 'layouts.count', { n: String(l.widgets) })}{l.toolbar ? ` · ${t(lang, 'layouts.withBar')}` : ''}
                {#if l.followsDofus}· {t(lang, 'layouts.followsDofus')}{/if}
                {#if view.active === i}<span class="badge">{t(lang, 'layouts.active')}</span>{/if}
              </span>
            </div>
            {#if l.tools.length > 0 || l.toolbar}
              <!-- Contenu : chaque élément peut être retiré de cette disposition. -->
              <ul class="items" aria-label={t(lang, 'layouts.contents')}>
                {#each l.tools as id (id)}
                  <li class="chip">
                    <span>{toolName(id)}</span>
                    <button
                      class="chip-x"
                      onclick={() => run(() => layoutRemoveItem(i, id))}
                      aria-label={t(lang, 'layouts.removeItem', { name: toolName(id) })}
                      title={t(lang, 'layouts.removeItem', { name: toolName(id) })}>×</button
                    >
                  </li>
                {/each}
                {#if l.toolbar}
                  <li class="chip bar">
                    <span>{t(lang, 'layouts.toolbarItem')}</span>
                    <button
                      class="chip-x"
                      onclick={() => run(() => layoutRemoveItem(i, LAYOUT_TOOLBAR))}
                      aria-label={t(lang, 'layouts.removeItem', { name: t(lang, 'layouts.toolbarItem') })}
                      title={t(lang, 'layouts.removeItem', { name: t(lang, 'layouts.toolbarItem') })}>×</button
                    >
                  </li>
                {/if}
              </ul>
            {/if}
            {#if confirmDelete === i}
              <div class="row confirm" role="alert">
                <span>{t(lang, 'layouts.confirmDelete', { name: l.name })}</span>
                <button
                  class="btn-line small danger"
                  onclick={() => {
                    confirmDelete = null;
                    run(() => layoutDelete(i));
                  }}>{t(lang, 'layouts.delete')}</button
                >
                <button class="text-link" onclick={() => (confirmDelete = null)}>{t(lang, 'shortcuts.cancel')}</button>
              </div>
            {:else}
              <div class="row tools">
                <button class="text-link" title={t(lang, 'layouts.overwrite')} onclick={() => run(() => layoutOverwrite(i))}>{t(lang, 'layouts.overwriteShort')}</button>
                <button
                  class="text-link"
                  onclick={() => {
                    renaming = i;
                    renameText = l.name;
                    setTimeout(() => dialog?.querySelector<HTMLInputElement>('input.rename')?.select());
                  }}>{t(lang, 'layouts.rename')}</button
                >
                <button class="icon" disabled={i === 0} onclick={() => run(() => layoutMove(i, i - 1))} aria-label={t(lang, 'layouts.up')} title={t(lang, 'layouts.up')}>↑</button>
                <button
                  class="icon"
                  disabled={i === view.layouts.length - 1}
                  onclick={() => run(() => layoutMove(i, i + 1))}
                  aria-label={t(lang, 'layouts.down')}
                  title={t(lang, 'layouts.down')}>↓</button
                >
                <button class="text-link danger del" onclick={() => (confirmDelete = i)}>{t(lang, 'layouts.delete')}</button>
              </div>
            {/if}
          </li>
        {/each}
      </ol>
    {/if}

    {#if !followDofus}<p class="note small-note serious">{t(lang, 'layouts.dofusNote')}</p>{/if}
{/snippet}

<style>
  .inline h2 {
    margin: 0 0 4px;
    font-size: 18px;
  }
  .items {
    list-style: none;
    margin: 6px 0 0;
    padding: 0;
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    flex-direction: row;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    padding: 1px 2px 1px 8px;
    font-size: 12.5px;
    border: 0;
    box-shadow: var(--sunk);
    background: var(--bg);
  }
  .chip.bar {
    font-style: italic;
  }
  .chip-x {
    border: 0;
    background: none;
    color: var(--text-muted);
    cursor: pointer;
    font-size: 15px;
    line-height: 1;
    padding: 2px 5px;
  }
  .chip-x:hover {
    color: var(--danger, #d46a5a);
  }
  .note {
    margin: 4px 0 12px;
    font-size: 13.5px;
    color: var(--text-muted);
  }
  .small-note {
    margin: 12px 0 0;
    font-size: 12.5px;
  }
  .new {
    display: flex;
    gap: 8px;
    margin-bottom: 12px;
  }
  input[type='text'] {
    flex: 1;
    min-width: 0;
    min-height: 32px;
    padding: 2px 8px;
    background: var(--bg-sunken);
    color: var(--text);
    border: 0;
    box-shadow: var(--sunk);
    font: inherit;
  }
  input[type='text']:focus {
    border-color: var(--accent);
  }
  .new .btn-plank {
    padding: 4px 16px;
    font-size: 14px;
  }
  .empty {
    color: var(--text-muted);
    text-align: center;
    margin: 18px 0;
  }
  ol {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  ol > li {
    padding: 8px 10px;
    border: 0;
    box-shadow: var(--sunk);
    background: var(--bg-sunken);
  }
  /* Disposition active : liseré vert mousse à gauche. */
  ol > li.active {
    box-shadow: var(--sunk), inset 3px 0 0 var(--green);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .num {
    font-weight: 800;
    color: var(--accent);
    font-size: 16px;
    width: 1.2em;
    text-align: center;
  }
  .name {
    flex: 1;
    min-width: 0;
    font-weight: 700;
    font-size: 15px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .rename {
    flex: 1;
  }
  kbd {
    font-family: inherit;
    font-weight: 700;
    padding: 0 7px;
    border: 1px solid var(--wood-light);
    border-bottom-width: 3px;
    border-radius: 2px;
    background: #2f2a20;
    font-size: 12.5px;
  }
  .small {
    padding: 1px 10px;
    font-size: 13.5px;
  }
  .meta {
    margin: 2px 0 0 calc(1.2em + 8px);
    font-size: 13px;
    color: var(--text-muted);
  }
  .badge {
    margin-left: 6px;
    padding: 0 6px;
    background: #3a4325;
    color: #d2dfa5;
    font-family: var(--font-title);
    font-weight: 700;
  }
  .tools,
  .confirm {
    flex-wrap: wrap;
    margin: 6px 0 0 calc(1.2em + 8px);
    gap: 4px 10px;
  }
  .confirm span {
    font-size: 13.5px;
  }
  .del {
    margin-left: auto;
  }
  .danger:hover {
    color: var(--danger);
    border-color: var(--danger);
  }
  .icon {
    border: 0;
    background:
      repeating-linear-gradient(0deg, #0000 0 4px, #00000026 4px 5px, #0000 5px 9px, #ffffff0a 9px 10px),
      linear-gradient(180deg, #6b4526, #4f321b 60%, #3f2714);
    box-shadow: inset 0 1px 0 #a0703f80, inset 0 -1px 0 #1d1209, 0 2px 0 #0c0e0a;
    color: #f6e6c6;
    width: 24px;
    height: 22px;
    font-size: 12px;
    padding: 0;
    line-height: 1;
  }
  .icon:hover:not(:disabled) {
    filter: brightness(1.18);
  }
</style>
