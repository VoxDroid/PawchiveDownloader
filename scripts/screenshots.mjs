// Takes the README screenshots from the interface running on its demo
// backend (sample data and drawn thumbnails, no real site content).
//
// Usage: pnpm screenshots   (starts `pnpm dev` itself if it is not running)
// Set PW_CHANNEL=msedge to use an installed Edge.
import { spawn } from 'node:child_process';
import { mkdirSync } from 'node:fs';
import { chromium } from '@playwright/test';

const BASE = 'http://localhost:1420';
const OUT = 'docs/screenshots';
mkdirSync(OUT, { recursive: true });

async function up() {
  try {
    return (await fetch(BASE)).ok;
  } catch {
    return false;
  }
}

let server = null;
if (!(await up())) {
  server = spawn('pnpm', ['dev'], { shell: true, stdio: 'ignore' });
  for (let i = 0; i < 60 && !(await up()); i++) await new Promise((r) => setTimeout(r, 500));
}

const browser = await chromium.launch(
  process.env.PW_CHANNEL ? { channel: process.env.PW_CHANNEL } : {},
);
const page = await browser.newPage({
  viewport: { width: 1280, height: 800 },
  deviceScaleFactor: 1.5,
});
await page.goto(BASE);
await page.getByRole('heading', { name: 'What would you like to download?' }).waitFor();

async function shot(name) {
  await page.waitForTimeout(700);
  await page.screenshot({ path: `${OUT}/${name}.png` });
  console.log(`${OUT}/${name}.png`);
}

async function open(url) {
  const bar = page.getByRole('textbox', { name: /Paste a Pawchive/ });
  await bar.fill(url);
  await bar.press('Enter');
}

async function nav(name) {
  await page
    .getByRole('navigation')
    .getByRole('link', { name: new RegExp(`^${name}(?![a-z])`) })
    .click();
}

await shot('browse');

await open('https://pawchive.pw/patreon/user/48151623');
await page.getByText('50 of 50 loaded').waitFor();
const rows = page.getByRole('option');
await rows.nth(1).click();
await rows.nth(2).click();
await rows.nth(4).click();
// The banner and the creator's name at the top, with the picked rows.
await page.evaluate(() => document.querySelector('main')?.scrollTo(0, 0));
await shot('creator');

await rows.first().dblclick();
await page.getByRole('listbox', { name: 'Files in this post' }).waitFor();
await shot('post');

await page.getByText(/^\d+ links$/).click();
await page.getByText('mega.nz').first().waitFor();
await shot('links');

await nav('Downloads');
await page
  .getByRole('article')
  .filter({ hasText: 'Spring Sketchbook' })
  .getByRole('button', { name: /Show \d+ items/ })
  .click();
await shot('downloads');

await page.keyboard.press('Control+b');
await page.waitForTimeout(400);
await shot('compact');
await page.keyboard.press('Control+b');
await page.waitForTimeout(300);

await nav('Watchlist');
await page
  .getByRole('button', { name: /Show new posts/ })
  .first()
  .click()
  .catch(() => undefined);
await shot('watchlist');

await nav('Favorites');
await shot('favorites');

await nav('History');
await shot('history');

await nav('Statistics');
await page.getByRole('button', { name: 'Check now' }).click();
await page.getByText(/files are missing/).waitFor();
await shot('statistics');

await nav('Library');
await page
  .getByText(/^\d+ images$/)
  .first()
  .waitFor();
await shot('library');

await page.locator('button.tile').nth(1).click();
await page.getByRole('toolbar', { name: 'Zoom' }).waitFor();
await shot('library-viewer');
await page.keyboard.press('Escape');
await page.getByRole('dialog').waitFor({ state: 'hidden' });

await nav('Browse');
await open('aster');
await page.getByRole('heading', { name: /Creators matching/ }).waitFor();
await shot('search');

await nav('Settings');
await shot('settings');

// Pairing with the browser extension, with a connected browser.
const bridge = page.locator('#s-bridge');
const accept = bridge.getByRole('switch', { name: 'Accept downloads from the browser' });
if ((await accept.getAttribute('aria-checked')) !== 'true') await accept.click();
await bridge.locator('.browsers').waitFor();
await bridge.evaluate((el) => el.scrollIntoView({ block: 'center' }));
await shot('pairing');
await page.evaluate(() => document.querySelector('main')?.scrollTo(0, 0));

await page.getByRole('button', { name: 'Light' }).click();
await nav('Browse');
await page.keyboard.press('Control+Shift+B');
await page
  .getByRole('dialog')
  .getByRole('textbox')
  .fill(
    'https://pawchive.pw/patreon/user/48151623\nhttps://pawchive.pw/fanbox/user/90210\nhttps://pawchive.pw/patreon/user/777001/post/175580042',
  );
await shot('batch-light');

await browser.close();
server?.kill();
process.exit(0);
