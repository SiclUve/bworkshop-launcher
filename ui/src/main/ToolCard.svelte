<script lang="ts">
  import type { InstallStatus, Lang, ProgressEvent, Tool } from '../shared/ipc';
  import { colon, t } from '../shared/i18n';
  import { translateError } from '../shared/errors';

  interface Props {
    tool: Tool;
    lang: Lang;
    favorite: boolean;
    status: InstallStatus;
    activity: ProgressEvent | undefined;
    onToggleFavorite: (id: string) => void;
    onOpenLink: (url: string) => void;
    onInstall: (id: string) => void;
    onCancel: (id: string) => void;
    onUninstall: (id: string) => void;
    onLaunch: (id: string) => void;
    onDismiss: (id: string) => void;
    /** Le widget de cet outil est ouvert. */
    widgetOpen: boolean;
    /** Icône en cache (adresse data:), sinon l'initiale. */
    icon: string | undefined;
    onWidget: (id: string, open: boolean) => void;
  }
  let {
    tool,
    lang,
    favorite,
    status,
    activity,
    onToggleFavorite,
    onOpenLink,
    onInstall,
    onCancel,
    onUninstall,
    onLaunch,
    onDismiss,
    widgetOpen,
    onWidget,
    icon,
  }: Props = $props();

  let confirmingUninstall = $state(false);

  const tagline = $derived(tool.tagline ? tool.tagline[lang] || tool.tagline.fr : '');
  const initial = $derived(tool.name.trim().charAt(0).toUpperCase());
  const links = $derived(
    [
      { url: tool.pageUrl, label: t(lang, 'card.page') },
      { url: tool.sourceUrl, label: t(lang, 'card.source') },
      { url: tool.virustotalUrl, label: t(lang, 'card.virustotal') },
    ].filter((l): l is { url: string; label: string } => !!l.url),
  );

  // Avant le téléchargement, seule l'extension est connue : un .exe est annoncé comme
  // installateur. Le launcher reconnaît le vrai type par la signature du fichier ; si ce
  // n'en est pas un (logiciel portable), la fiche l'indique (« Téléchargé : dossier ouvert »).
  const isInstaller = $derived(
    !!tool.download && /\.exe$/i.test(tool.download.url.split(/[?#]/)[0]),
  );
  const busy = $derived(
    !!activity && ['downloading', 'verifying', 'installing', 'uninstalling'].includes(activity.phase),
  );
  const percent = $derived(
    activity?.phase === 'downloading' && activity.total
      ? Math.min(100, Math.round((activity.received / activity.total) * 100))
      : null,
  );
  const installedVersion = $derived(
    status.state === 'installed' ? status.version : status.state === 'updateAvailable' ? status.installed : null,
  );

  function mo(bytes: number): string {
    return (bytes / 1048576).toFixed(1).replace('.', lang === 'fr' ? ',' : '.') + (lang === 'fr' ? ' Mo' : ' MB');
  }

  function retry() {
    if (!activity) return;
    if (activity.operation === 'uninstall') onUninstall(tool.id);
    else onInstall(tool.id);
  }
</script>

<article class="card frame" class:soon={!tool.available && status.state === 'notInstalled'} aria-label={tool.name}>
  <div class="medallion" class:has-icon={!!icon} aria-hidden="true">
    {#if icon}<img src={icon} alt="" />{:else}{initial}{/if}
  </div>

  <div class="body">
    {#if tool.category}<p class="kicker">{tool.category}</p>{/if}
    <div class="title-row">
      <h3>{tool.name}</h3>
      <button
        class="star"
        aria-pressed={favorite}
        aria-label={t(lang, favorite ? 'card.favorite.remove' : 'card.favorite.add', { name: tool.name })}
        title={t(lang, favorite ? 'card.favorite.remove' : 'card.favorite.add', { name: tool.name })}
        onclick={() => onToggleFavorite(tool.id)}
      >
        {favorite ? '★' : '☆'}
      </button>
    </div>

    <p class="meta">
      <span>{tool.author}</span>
      {#if status.state === 'updateAvailable'}
        <span class="upgrade">· {t(lang, 'card.upgrade', { from: status.installed, to: status.available })}</span>
      {:else if installedVersion}
        <span>· {t(lang, 'card.installed', { v: installedVersion })}</span>
      {/if}
      {#if tool.version && status.state === 'notInstalled'}
        <span>· {t(lang, 'card.available', { v: tool.version })}</span>
      {/if}
      {#if tool.status !== 'stable'}
        <span class="wood-tag">{t(lang, `status.${tool.status}`)}</span>
      {/if}
      {#if !tool.available}
        <span class="badge soon-badge">{t(lang, 'card.soon')}</span>
      {/if}
    </p>

    {#if tagline}<p class="tagline" title={tagline}>{tagline}</p>{/if}

    <!-- Progression d'une opération en cours -->
    {#if busy && activity}
      <div class="progress" role="status" aria-live="polite">
        <span>{t(lang, `phase.${activity.phase}` as 'phase.downloading')}</span>
        {#if activity.phase === 'downloading'}
          <div
            class="bar"
            role="progressbar"
            aria-valuemin="0"
            aria-valuemax="100"
            aria-valuenow={percent ?? undefined}
          >
            <div class="fill" class:indeterminate={percent === null} style:width={percent === null ? '30%' : `${percent}%`}></div>
          </div>
          <span class="bytes">
            {percent !== null ? `${percent} %` : mo(activity.received)}
          </span>
          <button class="btn-line small" onclick={() => onCancel(tool.id)}>{t(lang, 'action.cancel')}</button>
        {/if}
      </div>
    {:else if activity?.phase === 'downloaded'}
      <div class="alert" role="status">
        <p>{t(lang, 'phase.downloaded')}</p>
        <div class="actions">
          <button class="btn-line small" onclick={() => onDismiss(tool.id)}>{t(lang, 'action.close')}</button>
        </div>
      </div>
    {:else if activity?.phase === 'error'}
      <div class="alert" role="alert">
        <p><strong>{t(lang, 'phase.error')}{colon(lang).trimEnd()}</strong> {translateError(lang, activity.message ?? '')}</p>
        <div class="actions">
          <button class="btn-plank" onclick={retry}>{t(lang, 'action.retry')}</button>
          <button class="btn-line small" onclick={() => onDismiss(tool.id)}>{t(lang, 'action.close')}</button>
        </div>
      </div>
    {:else if confirmingUninstall}
      <div class="alert" role="alert">
        <p>{t(lang, 'confirm.uninstall', { name: tool.name })}</p>
        <div class="actions">
          <button
            class="btn-plank"
            onclick={() => {
              confirmingUninstall = false;
              onUninstall(tool.id);
            }}>{t(lang, 'action.confirmUninstall')}</button
          >
          <button class="btn-line small" onclick={() => (confirmingUninstall = false)}>{t(lang, 'action.cancel')}</button>
        </div>
      </div>
    {:else}
      <div class="actions">
        {#if status.state === 'notInstalled' && tool.available}
          <button class="btn-plank" onclick={() => onInstall(tool.id)}>
            {t(lang, isInstaller ? 'action.install' : 'action.downloadOpen')}
          </button>
        {/if}
        {#if status.state === 'updateAvailable' && tool.available}
          <button class="btn-plank" onclick={() => onInstall(tool.id)}>{t(lang, 'action.update')}</button>
        {/if}
        {#if status.state !== 'notInstalled'}
          <button class={status.state === 'installed' ? 'btn-plank' : 'btn-line small'} onclick={() => onLaunch(tool.id)}>
            {t(lang, 'action.launch')}
          </button>
        {/if}
        {#if tool.webUrl}
          <button
            class="btn-line web"
            aria-pressed={widgetOpen}
            title={t(lang, widgetOpen ? 'action.closeWidget' : 'action.openWidget')}
            onclick={() => onWidget(tool.id, !widgetOpen)}
          >
            {t(lang, widgetOpen ? 'action.closeWidgetShort' : 'action.openWidgetShort')}
          </button>
        {/if}
      </div>
    {/if}

    {#if links.length || status.state !== 'notInstalled'}
      <div class="links">
        {#each links as link (link.url)}
          <button class="text-link" onclick={() => onOpenLink(link.url)}>{link.label} ↗</button>
        {/each}
        {#if status.state !== 'notInstalled' && !confirmingUninstall && !busy}
          <button class="text-link danger uninstall" onclick={() => (confirmingUninstall = true)}
            ><span class="bw-ic bucket" aria-hidden="true"></span>{t(lang, 'action.uninstall')}</button
          >
        {/if}
      </div>
    {/if}
  </div>
</article>

<style>
  .card {
    display: flex;
    gap: var(--s3);
    align-items: flex-start;
    padding: var(--s3) 14px;
  }
  .card.soon {
    opacity: 0.65;
  }
  .medallion {
    flex: none;
    width: 44px;
    height: 44px;
    display: grid;
    place-items: center;
    background: radial-gradient(circle at 35% 30%, #a8743d, #6e4a26 70%);
    border: 2px solid #3b2512;
    box-shadow: inset 0 0 0 2px rgba(255, 220, 160, 0.18), 0 3px 0 #0d0f0a;
    color: #fff1d4;
    font-family: var(--font-paint);
    font-size: 28px;
  }
  .medallion.has-icon {
    background: transparent;
    border-color: transparent;
    box-shadow: none;
  }
  .medallion img {
    width: 100%;
    height: 100%;
    object-fit: contain;
    filter: drop-shadow(0 2px 2px rgba(0, 0, 0, 0.5));
  }
  .body {
    flex: 1;
    min-width: 0;
  }
  .kicker {
    margin: 0 0 2px;
    font-size: 13px;
  }
  .title-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  h3 {
    flex: 1;
    min-width: 0;
    font-size: 20px;
    color: var(--text);
    line-height: 1.15;
  }
  .star {
    border: none;
    background: transparent;
    color: var(--accent);
    font-size: 22px;
    line-height: 1;
    padding: 2px 4px;
  }
  .star:hover {
    color: var(--accent-hover);
  }
  .meta {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    margin: 3px 0 0;
    font-size: 14.5px;
    color: var(--text-muted);
  }
  .upgrade {
    color: var(--accent);
  }
  .badge {
    padding: 0 8px;
    font-weight: 700;
    font-size: 13px;
  }
  .soon-badge {
    background: #504c3b;
    color: #f3e9cb;
  }
  .meta .wood-tag {
    margin-left: 4px;
  }
  /* Deux lignes au plus : la fiche complète est sur le site. */
  .tagline {
    margin: 6px 0 0;
    font-size: 15px;
    line-height: 1.45;
    color: #e6dcc4;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--s2);
    margin-top: 10px;
  }
  .web {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .progress {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 12px;
    font-size: 13.5px;
  }
  .bar {
    flex: 1;
    height: 10px;
    background: var(--bg-sunken);
    border: 0;
    box-shadow: var(--sunk);
    overflow: hidden;
  }
  .fill {
    height: 100%;
    background: linear-gradient(90deg, #a6c24a, var(--green));
    transition: width 0.2s linear;
  }
  .fill.indeterminate {
    animation: slide 1.4s ease-in-out infinite;
  }
  @keyframes slide {
    from {
      transform: translateX(-100%);
    }
    to {
      transform: translateX(340%);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .fill.indeterminate {
      animation: none;
    }
  }
  .bytes {
    min-width: 3.5em;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .alert {
    margin-top: 12px;
    padding: 8px 10px;
    border-left: 4px solid var(--accent);
    background: var(--bg-sunken);
    font-size: 14px;
  }
  .alert p {
    margin: 0;
  }
  .alert .actions {
    margin-top: 8px;
  }
  /* Séparateur : baguette de bois, comme sur les cartes du site. */
  .links {
    position: relative;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px 14px;
    margin-top: 12px;
    padding-top: 9px;
  }
  .links::before {
    content: '';
    position: absolute;
    left: -4px;
    right: -4px;
    top: 0;
    height: 4px;
    background: linear-gradient(#bc8747, #6b4223 45%, #936232 55%, #4c311d);
    box-shadow: 0 2px 0 #0c100b;
  }
  .uninstall {
    --ic: 18px;
    display: inline-flex;
    align-items: center;
    gap: 3px;
    margin-left: auto;
  }
</style>
