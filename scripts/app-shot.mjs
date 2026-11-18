// Drives the running desktop app through WebView2's remote debugging port
// and takes screenshots. Start the app with
//   WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222 pnpm tauri dev
// then:
//   node scripts/app-shot.mjs <out.png> [#route] [js-to-run-before]
import { chromium } from '@playwright/test';

const [out, route, script] = process.argv.slice(2);
const browser = await chromium.connectOverCDP('http://127.0.0.1:9222');
const page = browser.contexts().flatMap((c) => c.pages())[0];
if (!page) throw new Error('No app page found');
if (route) {
  await page.evaluate((hash) => {
    location.hash = hash;
    window.dispatchEvent(new PopStateEvent('popstate'));
  }, route);
  await page.waitForTimeout(700);
}
if (script) {
  console.log(await page.evaluate(script));
  await page.waitForTimeout(1500);
}
if (out) {
  await page.screenshot({ path: out });
  console.log(`saved ${out}`);
}
await browser.close().catch(() => undefined);
