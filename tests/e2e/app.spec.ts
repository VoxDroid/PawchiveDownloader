import { expect, test, type Page } from '@playwright/test';

const CREATOR = 'https://pawchive.pw/patreon/user/48151623';

async function openLink(page: Page, url: string) {
  const bar = page.getByRole('textbox', { name: /Paste a Pawchive link/ });
  await bar.fill(url);
  await bar.press('Enter');
}

test.beforeEach(async ({ page }) => {
  await page.goto('/');
  await expect(
    page.getByRole('heading', { name: 'What would you like to download?' }),
  ).toBeVisible();
});

test('every page opens from the sidebar', async ({ page }) => {
  const pages: Array<[string, string]> = [
    ['Downloads', 'Downloads'],
    ['History', 'History'],
    ['Watchlist', 'Watchlist'],
    ['Favorites', 'Favorites'],
    ['Statistics', 'Statistics'],
    ['Logs', 'Logs'],
    ['Settings', 'Settings'],
    ['Help', 'Help'],
  ];
  for (const [link, heading] of pages) {
    // Badges (running jobs, new posts) are part of a link's name.
    await page
      .getByRole('navigation')
      .getByRole('link', { name: new RegExp(`^${link}`) })
      .click();
    await expect(page.getByRole('heading', { level: 1, name: heading })).toBeVisible();
  }
  await page.keyboard.press('Control+1');
  await expect(page).toHaveURL(/#browse$/);
});

test('a creator can be browsed, filtered and queued', async ({ page }) => {
  await openLink(page, CREATOR);
  await expect(page.getByRole('heading', { name: 'Aster Vale' })).toBeVisible();
  await expect(page.getByText('50 of 50 loaded')).toBeVisible();
  await page.getByRole('textbox', { name: 'Filter by title or ID' }).fill('Lighthouse');
  const rows = page.getByRole('option');
  await expect(rows.first()).toContainText('Lighthouse');
  await rows.first().click();
  await rows.nth(1).click({ modifiers: ['Shift'] });
  await expect(page.getByRole('button', { name: /Download 2 posts/ })).toBeVisible();
  await page.getByRole('button', { name: /Download 2 posts/ }).click();
  await expect(page.getByText('Added 2 posts to the queue')).toBeVisible();
  await page.getByRole('button', { name: 'Load 50 more' }).click();
  await expect(page.getByText(/of 100 loaded/)).toBeVisible();
});

test('a post shows its files and text', async ({ page }) => {
  await openLink(page, CREATOR);
  await page.getByRole('option').first().dblclick();
  const files = page.getByRole('listbox', { name: 'Files in this post' });
  await expect(files).toBeVisible();
  const first = files.getByRole('option').first();
  await expect(first).toHaveAttribute('aria-selected', 'true');
  await first.click();
  await expect(first).toHaveAttribute('aria-selected', 'false');
  await page.getByRole('radio', { name: 'Text' }).click();
  await expect(page.getByText('Thank you all for the support this month!')).toBeVisible();
});

test('the same download is not queued twice', async ({ page }) => {
  await openLink(page, CREATOR);
  await page.getByRole('option').first().dblclick();
  await page.getByRole('listbox', { name: 'Files in this post' }).waitFor();
  const download = page.getByRole('button', { name: /^Download \d+ files?$/ });
  await download.click();
  await expect(page.getByText(/^Added \d+ files? to the queue$/)).toBeVisible();
  await download.click();
  await expect(page.getByText('This download is already in the queue.')).toBeVisible();
});

test('the batch dialog counts links', async ({ page }) => {
  await page.keyboard.press('Control+Shift+B');
  const dialog = page.getByRole('dialog', { name: 'Queue many links' });
  await expect(dialog).toBeVisible();
  await dialog
    .getByRole('textbox')
    .fill(`${CREATOR}\nhttps://pawchive.pw/fanbox/user/90210/post/1\nnot a link`);
  await expect(dialog.getByText('1 creator')).toBeVisible();
  await expect(dialog.getByText('1 post')).toBeVisible();
  await dialog.getByRole('button', { name: 'Queue 2 links' }).click();
  await expect(page.getByText(/to the queue/)).toBeVisible();
});

test('jobs can be paused and resumed', async ({ page }) => {
  await page.getByRole('link', { name: /^Downloads/ }).click();
  const job = page.getByRole('article').filter({ hasText: 'Spring Sketchbook' });
  await job.getByRole('button', { name: 'Pause' }).click();
  await expect(job.getByText('Paused')).toBeVisible();
  await job.getByRole('button', { name: 'Resume' }).click();
  await expect(job.getByRole('button', { name: 'Pause' })).toBeVisible();
  await job.getByRole('button', { name: /Show \d+ items/ }).click();
  await expect(job.getByText('forest-01.png')).toBeVisible();
});

test('settings change the theme and the language', async ({ page }) => {
  await page.keyboard.press('Control+,');
  await page.getByRole('button', { name: 'Plum' }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'plum');
  await page.getByRole('combobox', { name: 'Language' }).selectOption('ja');
  await expect(page.getByRole('navigation').getByRole('link', { name: 'ブラウズ' })).toBeVisible();
  await expect(page.locator('html')).toHaveAttribute('lang', 'ja');
});

test('the help guide can be searched', async ({ page }) => {
  await page.getByRole('link', { name: 'Help', exact: true }).click();
  await page.getByRole('textbox', { name: 'Search the guide' }).fill('socks5');
  await expect(page.getByRole('button', { name: 'Proxy and Tor' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Favorites' })).toHaveCount(0);
});

test('favorites can be edited', async ({ page }) => {
  await page.getByRole('link', { name: 'Favorites', exact: true }).click();
  const row = page.getByRole('article').filter({ hasText: 'Brush Pack' });
  await row.getByRole('button', { name: 'Edit favorite' }).click();
  const dialog = page.getByRole('dialog', { name: 'Edit favorite' });
  await dialog.getByRole('textbox', { name: 'Title' }).fill('Brushes');
  await dialog.getByRole('button', { name: 'Save' }).click();
  await expect(page.getByRole('button', { name: 'Brushes' })).toBeVisible();
});

test('plain words search creators and open the one picked', async ({ page }) => {
  await openLink(page, 'moss');
  await expect(page.getByRole('heading', { name: 'Creators matching "moss"' })).toBeVisible();
  await page.getByRole('button', { name: /Moss and Marrow/ }).click();
  await expect(page.getByRole('heading', { name: 'Moss and Marrow' })).toBeVisible();
});

test('a creator download can carry its own filters', async ({ page }) => {
  await openLink(page, CREATOR);
  await page.getByRole('button', { name: 'Filters' }).click();
  const dialog = page.getByRole('dialog');
  const skip = dialog.getByRole('textbox', { name: 'Skip posts with' });
  await skip.fill('wip');
  await skip.press('Enter');
  await expect(dialog.getByRole('button', { name: 'Remove wip' })).toBeVisible();
  await dialog.getByRole('button', { name: 'Close' }).last().click();
  await expect(page.getByRole('button', { name: /Filters\s*1/ })).toBeVisible();
});

test('statistics list missing files and offer to fetch them again', async ({ page }) => {
  await page.getByRole('link', { name: /Statistics/ }).click();
  await expect(page.getByRole('heading', { name: 'Most downloaded creators' })).toBeVisible();
  await page.getByRole('button', { name: 'Check now' }).click();
  await expect(page.getByText('3 files are missing, in 2 posts.')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Download them again' })).toBeVisible();
});

test('images open in the viewer and can be selected there', async ({ page }) => {
  await openLink(page, `${CREATOR}/post/175580025`);
  await page.getByRole('button', { name: 'View' }).first().click();
  const viewer = page.getByRole('dialog');
  await expect(viewer.getByText(/1 \/ \d+/)).toBeVisible();
  // Every file starts selected; the viewer can take one out and put it back.
  await viewer.getByRole('button', { name: 'Selected' }).click();
  await expect(viewer.getByRole('button', { name: 'Select', exact: true })).toBeVisible();
  await page.keyboard.press('Space');
  await expect(viewer.getByRole('button', { name: 'Selected' })).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(viewer).toBeHidden();
});

test('recent posts load and stay loaded', async ({ page }) => {
  await page.getByRole('button', { name: /Recent posts/ }).click();
  await expect(page.getByRole('heading', { name: 'Recent posts' })).toBeVisible();
  const loading = page.getByText('Loading...');
  await expect(loading).toBeHidden({ timeout: 5000 });
  // It used to start over after every page, forever.
  await page.waitForTimeout(1500);
  await expect(loading).toBeHidden();
});
