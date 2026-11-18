# Translating

The app speaks English, Japanese, Korean, Simplified Chinese, Spanish,
Brazilian Portuguese, Russian, German and French. Translations are welcome,
for new languages and for improvements.

## Where the text lives

- `src/lib/i18n/locales/<locale>.json` are the dictionaries the app loads.
  They are generated; do not edit them by hand.
- `scripts/i18n-src/<locale>.json` are the sources for messages added for the
  desktop app. Messages carried over from the browser extension are kept in
  the dictionaries as they are.
- The tray menu and notifications are translated in Rust, in the `words`
  function of `src-tauri/src/app.rs`.

## Plural forms

In a source file, a message with a count is written once:

- a string for languages with one form (Japanese, Korean, Chinese),
- `[one, other]` for English, Spanish, Portuguese, German and French,
- `[one, few, many]` for Russian.

The merge script expands them into the CLDR keys (`_one`, `_few`, `_many`,
`_other`) the app looks up.

## Workflow

1. Edit or add messages in `scripts/i18n-src/<locale>.json`. Keep every
   `{placeholder}` from the English text.
2. Run `node scripts/merge-locale.mjs en ja ko zh_CN es pt_BR ru de fr`.
3. Run `pnpm lint:locales`. It checks that every language has every message,
   every plural form its language needs and every placeholder, and that every
   key the code uses exists.
4. Look at the result: `pnpm dev`, then Settings > Language.

## Style

- Write naturally; do not translate word for word.
- Keep it short: labels and buttons have limited room.
- Use the platform's usual words (the tray is "zone de notification" in French,
  "Infobereich" in German).
- No emojis.

## Adding a language

1. Add the locale to `LOCALES` in `src/lib/types.ts` and
   `src-tauri/src/settings.rs`, and its name to `LANGUAGE_NAMES` and `BCP47` in
   `src/lib/i18n/index.ts`.
2. Copy `src/lib/i18n/locales/en.json` to the new locale and translate it, and
   create its source file in `scripts/i18n-src/`.
3. Add its words to `src-tauri/src/app.rs`, its tag to `scripts/check-locales.mjs`,
   and run the checks above.
