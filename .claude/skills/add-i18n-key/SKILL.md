---
name: add-i18n-key
description: Add a new localized UI string the correct way — a Key enum variant, its (en, ja) entry, and a Key::ALL registration — so the every_key_has_both_languages test passes. Use whenever introducing or changing any user-facing text; never hardcode UI strings.
disable-model-invocation: true
---

# Add an i18n key

**Never hardcode UI strings.** All user-facing text is keyed by the `Key` enum in
`src/i18n/mod.rs` and routed through `t()` (static) or `tf()` (positional `{}`
substitution). Adding a string is three edits in one file, plus the call site.

## Steps (all in `src/i18n/mod.rs` unless noted)

1. **Add the variant** to the `Key` enum. Place it in the relevant comment-grouped
   section (e.g. `// Detail pane.`, `// Modal hints / bodies.`) for readability.

2. **Register it in `Key::ALL`** (the `pub const ALL: &'static [Key]` slice). This is
   easy to forget and is exactly what the `every_key_has_both_languages` test guards —
   a missing entry fails the test, a missing translation panics `entry()`.

3. **Add the translation** in `fn entry(key: Key) -> (&'static str, &'static str)`.
   Return `(english, japanese)` — **both are mandatory**. For dynamic text use `{}`
   placeholders, substituted positionally by `tf` in call-site order:
   ```rust
   Key::FilterApplied => ("filter: {} · Esc clears", "フィルタ: {} · Esc で解除"),
   ```

4. **Use it at the call site** via `t(lang, Key::Foo)` for static text or
   `tf(lang, Key::Foo, &[arg1, arg2])` for substitution. Keep `{}` count and the
   args array length in sync. UI code lives in `src/ui/mod.rs`; success/error message
   text often lives near `message.rs` (`done_message`) or the reducer.

## Verify

```sh
cargo test every_key_has_both_languages   # enforces ALL ⊇ every variant, both langs filled
cargo fmt --all
cargo clippy --all-targets -- -D warnings
```

## Gotchas

- Japanese is required, not optional — a placeholder English string in the `ja` slot
  defeats the feature. Translate it.
- Keep placeholder counts identical across `en` and `ja`; `tf` substitutes the same
  args into both, so a mismatched `{}` count produces a malformed string in one language.
- Width is CJK-aware in the UI (`unicode-width`); don't assume `ja` text occupies the
  same column count as `en` when reasoning about layout.
