<script lang="ts">
  // Visite guidée : cinq bulles au premier lancement, puis « Aide » pour la revoir.
  import { onMount } from 'svelte';
  import type { Lang } from '../shared/ipc';
  import { t } from '../shared/i18n';

  interface Props {
    lang: Lang;
    /** Raccourcis actuels, montrés tels que l'utilisateur les a réglés. */
    panelKey: string | null;
    lockKey: string | null;
    onDone: () => void;
  }
  let { lang, panelKey, lockKey, onDone }: Props = $props();

  const STEPS = ['welcome', 'toolbar', 'hotkey', 'web', 'layouts'] as const;
  let step = $state(0);
  let card: HTMLDivElement | undefined = $state();

  const key = (k: string | null) => k ?? t(lang, 'tour.noKey');

  onMount(() => card?.querySelector<HTMLButtonElement>('.next')?.focus());

  function next() {
    if (step < STEPS.length - 1) step += 1;
    else onDone();
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape') onDone();
    else if (e.key === 'ArrowRight') next();
    else if (e.key === 'ArrowLeft' && step > 0) step -= 1;
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="backdrop">
  <div class="card" role="dialog" aria-modal="true" aria-labelledby="tour-title" aria-describedby="tour-text" bind:this={card}>
    <p class="count">{step + 1} / {STEPS.length}</p>
    <h2 id="tour-title">{t(lang, `tour.${STEPS[step]}.title`)}</h2>
    <p id="tour-text">
      {t(lang, `tour.${STEPS[step]}.text`, { panel: key(panelKey), lock: key(lockKey) })}
    </p>
    <div class="dots" aria-hidden="true">
      {#each STEPS as s, i (s)}<span class:on={i === step}></span>{/each}
    </div>
    <div class="buttons">
      <button class="skip" onclick={onDone}>{t(lang, 'tour.skip')}</button>
      {#if step > 0}<button onclick={() => (step -= 1)}>{t(lang, 'tour.back')}</button>{/if}
      <button class="next" onclick={next}>{t(lang, step === STEPS.length - 1 ? 'tour.done' : 'tour.next')}</button>
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(10, 12, 9, 0.72);
    display: grid;
    place-items: center;
    padding: 16px;
    z-index: 20;
  }
  .card {
    width: 100%;
    max-width: 360px;
    background: var(--parchment);
    color: var(--ink);
    border: 3px solid var(--wood);
    border-radius: 0;
    padding: 14px 18px 12px;
    box-shadow: 0 6px 24px rgba(0, 0, 0, 0.5);
  }
  .count {
    margin: 0;
    font-size: 12px;
    color: #5a4a33;
  }
  h2 {
    font-size: 26px;
    color: var(--ink);
    margin: 2px 0 6px;
  }
  #tour-text {
    margin: 0 0 12px;
    font-size: 15px;
    line-height: 1.5;
  }
  .dots {
    display: flex;
    gap: 6px;
    justify-content: center;
    margin-bottom: 10px;
  }
  .dots span {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: #b9a682;
  }
  .dots span.on {
    background: var(--wood);
  }
  .buttons {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .buttons button {
    border: 2px solid var(--wood);
    background: transparent;
    color: var(--ink);
    border-radius: 0;
    padding: 3px 12px;
    font-size: 14px;
  }
  .skip {
    margin-right: auto;
    border-color: transparent !important;
    text-decoration: underline;
  }
  .next {
    background: var(--wood) !important;
    color: var(--parchment) !important;
    font-weight: 700;
  }
  button:focus-visible {
    outline: 2px solid var(--ink);
    outline-offset: 2px;
  }
</style>
