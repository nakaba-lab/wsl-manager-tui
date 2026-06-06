# Managed export/import folder — design spec

Date: 2026-06-07
Status: Approved (pending written-spec review)
Component: `wslm` (wsl-manager-tui)

## 1. Motivation

Today, export (`e`) and import (`m`) make the user type full filesystem paths every
time: the export `.tar` path, and for import the new name, the install directory,
and the source `.tar`. This is tedious and error-prone.

This feature introduces a **managed folder** that owns both the export tarballs and
the install locations of imported distros, so the TUI can offer **selection-based**
export/import with minimal typing.

## 2. Goals / non-goals

**Goals**
- A single configurable root folder that holds export tarballs and the on-disk
  files (vhdx) of distros imported through `wslm`.
- Export: one confirmation dialog, edit just the filename, saved into the folder.
- Import: pick a tarball from a list; type only the new distro name; the install
  directory is chosen automatically.
- Allow importing a tarball from outside the managed folder (custom path).
- Allow deleting old tarballs from the import picker (housekeeping).

**Non-goals (v1, YAGNI)**
- A catalog/metadata file (source distro, timestamps) — the filesystem is the index.
- Auto-removing `installed\<name>\` when a distro is unregistered.
- Type-to-filter inside the import picker (lists are short; can be added later).
- Managing the install location of distros **not** created through `wslm`.
- Adding a date/time crate dependency (see §6 for the timestamp approach).

## 3. Managed folder layout & configuration

```
<manage_dir>\                         default: %USERPROFILE%\wsl-manager
  exports\                            export tarballs
    Ubuntu-20260607-153012.tar
    Debian-20260605-090145.tar
  installed\                          install locations (vhdx) for wslm imports
    MyUbuntu\    ext4.vhdx
    work-debian\ ext4.vhdx
```

- New preference `manage_dir: Option<PathBuf>` in `src/prefs/mod.rs`
  (`#[serde(default)]`, `None` ⇒ default). Resolver `Prefs::manage_dir() -> PathBuf`
  returns the configured path or `%USERPROFILE%\wsl-manager`.
- `exports\` and `installed\<name>\` are created on demand (`create_dir_all`) by the
  runtime immediately before an export/import runs. No directories are created at
  startup.

## 4. Architecture

Chosen approach: **idiomatic MVU integration** (vs. a minimal inline variant that
scatters path logic and is harder to test). It keeps the lib's layering: pure,
terminal-/IO-independent logic in `app`/`manage`; all IO described by `Command`s and
performed by the `runtime`.

### 4.1 New module `src/manage/mod.rs`

Pure helpers (unit-tested, no IO):
- `exports_dir(root: &Path) -> PathBuf` → `root/exports`
- `installed_dir(root: &Path, name: &str) -> PathBuf` → `root/installed/<sanitized>`
- `export_filename(distro: &str, when: SystemTime) -> String`
  → `"<distro>-<YYYYMMDD-HHMMSS>.tar"` (UTC; see §6)
- `derive_distro_name(tar_filename: &str) -> String` → filename without extension,
  trailing `-<timestamp>` left intact for the user to edit
- `sanitize_name(name: &str) -> String` → strip path separators / illegal chars

Filesystem helpers (synchronous; the runtime calls these via
`tokio::task::spawn_blocking`, mirroring `config::load_wslconfig`):
- `list_exports(root: &Path) -> io::Result<Vec<TarEntry>>` — `*.tar` in `exports\`,
  sorted newest-first by mtime; missing dir ⇒ `Ok(vec![])`
- `delete_export(path: &Path) -> io::Result<()>`
- `ensure_dirs(root: &Path) -> io::Result<()>` — create `exports\` (and `installed\`)

```rust
pub struct TarEntry {
    pub name: String,     // file name, e.g. "Ubuntu-20260607-153012.tar"
    pub path: PathBuf,    // absolute path
    pub size: u64,        // bytes (shown via ui::util::human_size)
}
```
(Modified-time is used only for sorting; it is not displayed in v1, so no date
formatting is needed.)

### 4.2 MVU additions (`src/app/`)

New `Command` variants (`message.rs`):
- `OpenExportDialog { distro: String }` — runtime computes the timestamped default
  filename and ensures `exports\` exists.
- `ListExports` — runtime lists `exports\`.
- `DeleteExport(PathBuf)` — runtime deletes a tarball, then re-lists.

Reused `Command`s: `Export { name, path }`, `Import { name, dir, tar }`. The runtime
`create_dir_all`s `dir` before importing (covers both managed and custom flows).

New `Action` variants:
- `ExportDialogReady { distro: String, filename: String }`
- `ExportsListed(Vec<TarEntry>)`

New `Modal` variant: `Modal::ImportPick(ImportPickState)`.

```rust
pub struct ImportPickState { pub entries: Vec<TarEntry>, pub selected: usize }
// select_next / select_prev (clamped), selected_entry() -> Option<&TarEntry>
```

New `FormKind`s (`modal.rs`):
- `FormKind::ImportName { tar: PathBuf }` — one field: new distro name.
- `FormKind::ImportCustom` — two fields: tar path, new distro name.

Export keeps `FormKind::Export { distro }` but its single field now means **filename**
(not a full path); the command builds `exports_dir(root).join(filename)`.

The existing `FormKind::Import` (name + dir + tar) is **removed**; `open_import_form`
now emits `ListExports` instead of opening that form.

### 4.3 Where does `manage_dir` (root) come from inside the pure reducer?

The reducer must stay IO-free, but it needs the root to build `Import`/`Export`
paths. `manage_dir` is resolved once at startup and stored on the `Model`
(`Model.manage_dir: PathBuf`, set in `runtime::run` from `Prefs::manage_dir()`,
just like `lang`/`keybind_style`). The reducer reads `model.manage_dir`; it never
touches the filesystem.

## 5. Flows

### 5.1 Export (`e`)
1. `e` on the list → `Command::OpenExportDialog { distro }` (needs a selection).
2. Runtime: `ensure_dirs`, build `filename = export_filename(distro, now)` →
   `Action::ExportDialogReady { distro, filename }`.
3. Reducer opens `Modal::Form(Export)` with the filename field pre-filled; the UI
   shows the fixed `→ <manage_dir>\exports\` prefix as context.
4. Enter (non-empty) → `Modal::Progress` + `Command::Export { name: distro,
   path: exports_dir(root).join(filename) }`. Esc cancels.
5. `OpDone`/`OpFailed` as today. No overwrite check (timestamped default makes
   collisions effectively impossible; editing to an existing name overwrites,
   matching `wsl --export` behaviour).

### 5.2 Import (`m`)
1. `m` → `Command::ListExports` → `Action::ExportsListed` → `Modal::ImportPick`.
2. Picker keys:
   - `↑`/`↓` (and `j`/`k` per `keybind_style`) — move selection
   - `Enter` — choose the selected tarball → open `Modal::Form(ImportName { tar })`
     with the name field pre-filled from `derive_distro_name`
   - `c` — custom tarball: open `Modal::Form(ImportCustom)` (tar path + name)
   - `d` — delete the selected tarball → `Modal::Confirm` → `Command::DeleteExport`
   - `Esc` — close the picker
   Footer hint lists these. Empty list shows an "(empty — press `c`)" message; only
   `c`/`Esc` are actionable.
3. Name form submit (`ImportName`/`ImportCustom`):
   - Validate required fields non-empty (else keep the form open, as today).
   - If a registered distro already has that name (case-insensitive, checked against
     `model.distros` — pure), show the existing `PromptImportOverwrite` confirm.
   - Otherwise → `Modal::Progress` + `Command::Import { name, dir:
     installed_dir(root, name), tar }`.
4. Runtime `create_dir_all(dir)` before `backend.import`; `OpDone`/`OpFailed` as today.

### 5.3 Delete (`d` in the picker)
`d` → `Modal::Confirm` (prompt names the file) → `Command::DeleteExport(path)`.
On success the runtime re-runs `ListExports` and re-emits `ExportsListed`, so the
picker reopens refreshed (selection clamped); a status message reports the deletion.
On failure → `OpFailed` modal.

## 6. Timestamp handling (design decision)

`export_filename` needs a timestamp at dialog-open time. The reducer is pure (no
clock), so the **runtime** supplies it: `OpenExportDialog` → runtime reads
`SystemTime::now()` → `ExportDialogReady`. This mirrors the existing async-open
patterns (`ConfigLoaded`, `OnlineList`).

To avoid adding a date crate, `export_filename` formats the timestamp as **compact
UTC** `YYYYMMDD-HHMMSS` using a small, dependency-free civil-date conversion
(Howard Hinnant's algorithm) over `duration_since(UNIX_EPOCH)`. It is a pure
function of the passed `SystemTime`, so it is deterministic and unit-tested with
fixed inputs. UTC (not local) is acceptable because the value is only a default
filename the user can edit, and it remains sortable.

Alternative considered: add the `time`/`chrono` crate for local-time formatting —
rejected for v1 to keep dependencies minimal. (Revisit if local time is desired.)

## 7. i18n

Add `Key`s (en/ja both filled; keep `Key::ALL` in sync — enforced by
`every_key_has_both_languages`):
- `FormImportNameTitle`, `LabelImportNameOnly`, `LabelImportCustomTar`
- `PickImportTitle`, `PickImportEmpty`, `PickImportHints` (Enter/​c/​d/​Esc)
- `ExportSavedToHint` (the `→ exports\` prefix label)
- `PromptDeleteTar`, `DoneDeletedTar`, `StatusListingExports`

Existing keys reused: `ProgExporting`, `ProgImporting`, `PromptImportOverwrite`,
`DoneExported`, `DoneImported`, `FormExportTitle`, `LabelExportPath` (relabelled to
"Output file name" / "出力ファイル名").

## 8. Error handling

- All `manage` fs errors surface through the existing `OpFailed` → `Modal::Error`.
- A missing/empty `exports\` is not an error: `list_exports` returns empty and the
  picker shows the empty hint.
- An unwritable `manage_dir` (mkdir/export/import failure) → `OpFailed` with the OS
  error message (localized wrapper `Key::FailOp`).

## 9. Testing

- **`manage` (pure):** `exports_dir`/`installed_dir`/`export_filename`
  (fixed `SystemTime` → exact string) / `derive_distro_name` / `sanitize_name`.
- **Reducer:** `e` → `OpenExportDialog`; `ExportDialogReady` opens the export form;
  export submit builds the correct `exports\…` path; `m` → `ListExports`;
  `ExportsListed` opens `ImportPick`; picker nav / `c` / `d`-confirm / `Enter` →
  name form; name submit → `Import { dir = installed\<name>\, tar }`; name collision
  → overwrite confirm; `DeleteExport` re-lists.
- **prefs:** TOML round-trip incl. `manage_dir`; default resolves when `None`.
- **UI (TestBackend):** `ImportPick` render (list + size + hints + empty state),
  export form (filename + prefix), import name/custom forms.
- `manage::list_exports`/`delete_export` against a `tempdir` may be `#[ignore]`d or
  use a temp path; pure path/format helpers are always tested.

## 10. File-by-file change summary

- `src/prefs/mod.rs` — add `manage_dir` field + `manage_dir()` resolver + test.
- `src/manage/mod.rs` — **new**: pure helpers + fs helpers + `TarEntry` + tests.
- `src/lib.rs` — register `manage` module (IO-independent layer).
- `src/app/model.rs` — add `manage_dir: PathBuf` to `Model`.
- `src/app/message.rs` — `Command::{OpenExportDialog, ListExports, DeleteExport}`;
  `Action::{ExportDialogReady, ExportsListed}`.
- `src/app/modal.rs` — `Modal::ImportPick`, `ImportPickState`; `FormKind::{ImportName,
  ImportCustom}`; remove `FormKind::Import`; `FormState` constructors.
- `src/app/update/mod.rs` — `e`/`m` handlers; new actions; export form open via
  `ExportDialogReady`.
- `src/app/update/modal.rs` — `handle_import_pick_key`; export/import submit paths.
- `src/runtime/mod.rs` — set `model.manage_dir`; dispatch `OpenExportDialog`,
  `ListExports`, `DeleteExport`; `create_dir_all(dir)` before `Import`.
- `src/ui/mod.rs` — render `ImportPick`; export filename prefix; import forms.
- `src/i18n/mod.rs` — new keys (en/ja) + `Key::ALL`.

## 11. Keybindings (summary)

- List: `e` export (unchanged), `m` import (now opens the picker).
- Import picker: `↑`/`↓`(+`j`/`k`) move · `Enter` import · `c` custom tar ·
  `d` delete · `Esc` back.
- Export dialog / import name dialog: edit field · `Enter` confirm · `Esc` cancel.
