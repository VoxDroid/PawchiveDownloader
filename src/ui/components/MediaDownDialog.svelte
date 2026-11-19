<script lang="ts">
  import { t } from '../state/app.svelte';
  import { answerMediaDown, mediaPrompt } from '../state/mediaPrompt.svelte';
  import Dialog from './Dialog.svelte';
  import Icon from './Icon.svelte';
</script>

<Dialog
  open={mediaPrompt.open}
  title={t('mediaDown.title', { site: mediaPrompt.site })}
  width={640}
  confirmLabel={t('mediaDown.thumbnails')}
  onconfirm={() => answerMediaDown('thumbnails')}
  secondaryLabel={t('mediaDown.continue')}
  onsecondary={() => answerMediaDown('continue')}
  oncancel={() => answerMediaDown('cancel')}
>
  <p class="lead"><Icon name="cloudOff" />{t('mediaDown.body', { host: mediaPrompt.host })}</p>
  <ul>
    <li><strong>{t('mediaDown.thumbnails')}:</strong> {t('mediaDown.thumbnailsHint')}</li>
    <li><strong>{t('mediaDown.continue')}:</strong> {t('mediaDown.continueHint')}</li>
  </ul>
</Dialog>

<style>
  .lead {
    display: flex;
    gap: 10px;
    margin: 0 0 12px;
    line-height: 1.5;
  }
  .lead :global(.icon) {
    flex: none;
    margin-top: 2px;
    color: var(--warning);
  }
  ul {
    margin: 0;
    padding-left: 18px;
    display: grid;
    gap: 6px;
    font-size: var(--fs-sm);
    color: var(--text-dim);
  }
</style>
