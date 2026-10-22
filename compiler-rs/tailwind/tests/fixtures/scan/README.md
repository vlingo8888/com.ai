# Scanner fixtures

Hand-written template files in many syntaxes. `worker/scan-parity.ts --write-fixtures`
records what `@tailwindcss/oxide` extracts from each into `../scan_expected.json`, and
`tests/scan_parity.rs` checks that `src/scan` extracts exactly the same candidates.

```html
<div class="flex items-center md:grid-cols-[1fr_auto]">
```
