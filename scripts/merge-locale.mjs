// Applies a compact translation source (scripts/i18n-src/<locale>.json) on
// top of a runtime dictionary in src/lib/i18n/locales/. New and changed
// messages come from the source; messages for features the desktop app does
// not have are dropped. Plural messages are written once:
//   - a string, for languages with a single form (ja, ko, zh_CN)
//   - [one, other] for en, de, es, pt_BR and fr ("many" reuses "other")
//   - [one, few, many] for ru ("other", used for fractions, reuses "few")
// Keys follow the order of en.json, new keys at the end.
//
// Usage: node scripts/merge-locale.mjs <locale> [<locale> ...]
import { existsSync, readFileSync, writeFileSync } from 'node:fs';

const OBSOLETE = [
  /^fab\./,
  /^panel\./,
  /^popup\./,
  /^select\./,
  /^cards\./,
  /^listing\.tip/,
  /^other\./,
  /^menu\./,
  /^corner\./,
  /^side\./,
  /^shortcut\.(toggle-panel|download-page)$/,
  /^settings\.(page|floatingButton|floatingButtonHint|corner|panelSide|cardSelectors|cardSelectorsHint|markDownloaded|markDownloadedHint|accessMissing|grant|zipSeparate|zipSeparateHint|changeShortcuts|openShortcuts|shortcutsManual|noShortcut)$/,
  /^skip\.too-large$/,
  /^conflict\.prompt$/,
  /^nav\.(connected|reconnecting)$/,
  /^about\.featurePanel$/,
];
const PLURAL = /_(zero|one|two|few|many|other)$/;

const dir = new URL('../src/lib/i18n/locales/', import.meta.url);
const srcDir = new URL('./i18n-src/', import.meta.url);

function forms(locale, base, value) {
  const categories = new Intl.PluralRules(locale.replace('_', '-')).resolvedOptions()
    .pluralCategories;
  if (typeof value === 'string')
    return Object.fromEntries(categories.map((c) => [`${base}_${c}`, value]));
  if (locale === 'ru') {
    const [one, few, many] = value;
    return {
      [`${base}_one`]: one,
      [`${base}_few`]: few,
      [`${base}_many`]: many,
      [`${base}_other`]: few,
    };
  }
  const [one, other] = value;
  const out = { [`${base}_one`]: one, [`${base}_other`]: other };
  if (categories.includes('many')) out[`${base}_many`] = other;
  return out;
}

function merge(locale, enSource) {
  const current = JSON.parse(readFileSync(new URL(`${locale}.json`, dir), 'utf8'));
  const sourceUrl = new URL(`${locale}.json`, srcDir);
  const source = existsSync(sourceUrl) ? JSON.parse(readFileSync(sourceUrl, 'utf8')) : {};
  const missing = Object.keys(enSource).filter((k) => !(k in source));
  if (missing.length) {
    console.error(`${locale}: missing ${missing.join(', ')}`);
    process.exitCode = 1;
  }
  const result = {};
  for (const [key, value] of Object.entries(current)) {
    if (OBSOLETE.some((re) => re.test(key))) continue;
    if (key.replace(PLURAL, '') in source) continue;
    result[key] = value;
  }
  for (const [base, value] of Object.entries(source)) {
    if (Array.isArray(value) || (locale !== 'en' && Array.isArray(enSource[base])))
      Object.assign(result, forms(locale, base, value));
    else result[base] = value;
  }
  return result;
}

const enSource = JSON.parse(readFileSync(new URL('en.json', srcDir), 'utf8'));
const en = merge('en', enSource);
const order = Object.keys(en);
for (const locale of process.argv.slice(2)) {
  const dict = locale === 'en' ? en : merge(locale, enSource);
  const sorted = {};
  for (const key of order) if (key in dict) sorted[key] = dict[key];
  for (const key of Object.keys(dict)) if (!(key in sorted)) sorted[key] = dict[key];
  writeFileSync(new URL(`${locale}.json`, dir), `${JSON.stringify(sorted, null, 2)}\n`);
  console.log(`${locale}: ${Object.keys(sorted).length} messages`);
}
