<script lang="ts">
  import { api, openUrl } from '../../lib/api';
  import { app, t } from '../../ui/state/app.svelte';
  import { go } from '../../ui/state/router.svelte';
  import type { IconName } from '../../ui/icons.generated';
  import Button from '../../ui/components/Button.svelte';
  import Icon from '../../ui/components/Icon.svelte';
  import Logo from '../../ui/components/Logo.svelte';
  import SupportCard from '../../ui/components/SupportCard.svelte';
  import { LINKS } from '../../lib/links';

  const version = $derived(app.info?.version ?? '');
  const platform = $derived(
    ({ windows: 'Windows', macos: 'macOS', linux: 'Linux' } as Record<string, string>)[
      app.info?.platform ?? ''
    ] ?? app.info?.platform,
  );
  let checking = $state(false);

  async function checkNow() {
    checking = true;
    app.update = await api.checkUpdates().catch(() => null);
    checking = false;
  }

  const FEATURES: Array<[IconName, string]> = [
    ['compass', 'about.featureBrowse'],
    ['downloadMultiple', 'about.featureCreators'],
    ['retry', 'about.featureResume'],
    ['folderZip', 'about.featureZip'],
    ['timer', 'about.featureRelay'],
    ['fileCheck', 'about.featureVerify'],
    ['rename', 'about.featureTemplates'],
    ['playlistPlus', 'about.featureBatch'],
    ['eye', 'about.featureWatch'],
    ['starOutline', 'about.featureFavorites'],
    ['tray', 'about.featureTray'],
    ['network', 'about.featureProxy'],
    ['history', 'about.featureHistory'],
    ['language', 'about.featureLanguages'],
  ];

  const STEPS = ['about.step1', 'about.step2', 'about.step3', 'about.step4'];
</script>

<div class="hero">
  <Logo size={72} />
  <div>
    <h1>{t('app.name')}</h1>
    <p class="tagline">{t('about.tagline')}</p>
    <div class="meta">
      <span class="pill mono-num">v{version}</span>
      <span class="pill">{platform} {app.info?.arch}</span>
      <span class="pill" class:official={app.info?.channel === 'official'}>
        {app.info?.channel === 'official' ? t('about.channelOfficial') : t('about.channelSource')}
        {#if app.info?.portable}<span class="sep">/</span>{t('about.portable')}{/if}
      </span>
      <span class="pill">MIT</span>
    </div>
  </div>
</div>

<section class="update" class:available={app.update?.available}>
  <Icon name={app.update?.available ? 'update' : 'checkCircle'} size={22} />
  <div>
    {#if app.update?.available}
      <strong>{t('about.updateAvailable', { version: app.update.latest ?? '' })}</strong>
      <span>{t('about.updateBody')}</span>
    {:else if app.update && !app.update.error}
      <strong>{t('about.upToDate')}</strong>
      <span>{t('about.upToDateBody', { version })}</span>
    {:else if app.update?.error}
      <strong>{t('about.updateUnknown')}</strong>
      <span>{app.update.error === 'no-release' ? t('about.noRelease') : app.update.error}</span>
    {:else}
      <strong>{t('about.updates')}</strong>
      <span>{t('about.updatesBody')}</span>
    {/if}
  </div>
  {#if app.update?.available}
    <Button variant="primary" icon="coffee" onclick={() => app.update && openUrl(app.update.url)}>
      {t('about.getUpdate')}
    </Button>
    <Button
      variant="ghost"
      icon="github"
      onclick={() => app.update && openUrl(app.update.sourceUrl)}
    >
      {t('about.buildSource')}
    </Button>
  {:else}
    <Button icon="sync" loading={checking} onclick={checkNow}>{t('about.checkNow')}</Button>
  {/if}
</section>

<div class="support"><SupportCard /></div>

<section>
  <h2>{t('about.start')}</h2>
  <ol class="steps">
    {#each STEPS as step, i (step)}
      <li><span class="num">{i + 1}</span><span>{t(step)}</span></li>
    {/each}
  </ol>
</section>

<section>
  <h2>{t('about.features')}</h2>
  <div class="features">
    {#each FEATURES as [icon, key] (key)}
      <div class="feature">
        <span class="icon-box"><Icon name={icon} /></span><span>{t(key)}</span>
      </div>
    {/each}
  </div>
</section>

<section class="notes">
  <div class="note">
    <h3><Icon name="shield" />{t('about.politeTitle')}</h3>
    <p>{t('about.politeBody')}</p>
  </div>
  <div class="note">
    <h3><Icon name="info" />{t('about.ethicsTitle')}</h3>
    <p>{t('about.ethicsBody')}</p>
  </div>
  <div class="note">
    <h3><Icon name="database" />{t('about.privacyTitle')}</h3>
    <p>{t('about.privacyBody')}</p>
  </div>
</section>

<section class="links">
  <Button icon="github" onclick={() => openUrl(LINKS.repo)}>{t('about.source')}</Button>
  <Button icon="bug" onclick={() => openUrl(LINKS.issues)}>{t('about.report')}</Button>
  <Button icon="book" onclick={() => go('help')}>{t('nav.help')}</Button>
  <Button variant="ghost" icon="external" onclick={() => openUrl(LINKS.extension)}>
    {t('about.extension')}
  </Button>
  <Button variant="ghost" icon="external" onclick={() => openUrl(LINKS.kemonoDownloader)}>
    KemonoDownloader
  </Button>
</section>

<p class="credits faint">{t('about.credits')}</p>

<style>
  .update {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 14px 18px;
    margin-bottom: 20px;
    border-radius: var(--radius-lg);
    border: 1px solid var(--border);
    background: var(--bg-elev);
  }
  .update > :global(.icon) {
    color: var(--success);
  }
  .update.available {
    border-color: color-mix(in srgb, var(--accent) 50%, var(--border));
    background: var(--accent-softer);
  }
  .update.available > :global(.icon) {
    color: var(--accent-strong);
  }
  .update div {
    flex: 1;
    display: grid;
    gap: 2px;
  }
  .update span {
    font-size: var(--fs-sm);
    color: var(--text-dim);
  }
  .support {
    margin-bottom: 28px;
  }
  .hero {
    display: flex;
    align-items: center;
    gap: 24px;
    padding: 28px;
    margin-bottom: 28px;
    border-radius: var(--radius-xl);
    border: 1px solid var(--border);
    background:
      radial-gradient(600px 200px at 0% 0%, var(--accent-soft), transparent), var(--bg-elev);
  }
  h1 {
    font-size: 28px;
    font-weight: 700;
    letter-spacing: -0.02em;
  }
  .tagline {
    color: var(--text-dim);
    margin: 6px 0 12px;
    max-width: 560px;
  }
  .meta {
    display: flex;
    gap: 6px;
  }
  .pill.official {
    color: var(--accent-strong);
    border-color: color-mix(in srgb, var(--accent) 45%, var(--border));
  }
  .pill {
    padding: 2px 10px;
    border-radius: 99px;
    border: 1px solid var(--border-strong);
    font-size: var(--fs-xs);
    font-weight: 600;
    color: var(--text-dim);
  }
  section {
    margin-bottom: 28px;
  }
  h2 {
    font-size: var(--fs-lg);
    font-weight: 700;
    margin: 0 4px 12px;
  }
  .steps {
    list-style: none;
    padding: 0;
    margin: 0;
    display: grid;
    gap: 8px;
  }
  .steps li {
    display: flex;
    gap: 12px;
    align-items: center;
    padding: 12px 14px;
    border-radius: var(--radius-md);
    background: var(--bg-elev);
    border: 1px solid var(--border);
  }
  .num {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    border-radius: 8px;
    background: var(--accent);
    color: var(--accent-contrast);
    font-weight: 700;
    font-size: var(--fs-sm);
    flex: none;
  }
  .features {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
    gap: 10px;
  }
  .feature {
    display: flex;
    gap: 12px;
    align-items: center;
    padding: 12px 14px;
    border-radius: var(--radius-md);
    background: var(--bg-elev);
    border: 1px solid var(--border);
    font-size: var(--fs-sm);
  }
  .icon-box {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    border-radius: 9px;
    background: var(--accent-softer);
    color: var(--accent-strong);
    flex: none;
  }
  .notes {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(280px, 1fr));
    gap: 12px;
  }
  .note {
    padding: 16px;
    border-radius: var(--radius-lg);
    background: var(--bg-elev);
    border: 1px solid var(--border);
  }
  h3 {
    display: flex;
    gap: 8px;
    align-items: center;
    font-size: var(--fs-md);
    font-weight: 700;
    margin-bottom: 8px;
  }
  h3 :global(.icon) {
    color: var(--accent-strong);
  }
  .note p {
    color: var(--text-dim);
    font-size: var(--fs-sm);
  }
  .links {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }
  .credits {
    font-size: var(--fs-xs);
  }
</style>
