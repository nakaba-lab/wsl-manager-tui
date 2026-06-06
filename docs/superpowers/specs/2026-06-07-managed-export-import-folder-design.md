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
export/import with minimal typing, and supports the archive formats modern WSL
provides (tar, tar.gz, tar.xz, vhd).

## 2. Goals / non-goals

**Goals**
- A single configurable root folder that holds export archives and the on-disk
  files (vhdx) of distros imported through `wslm`.
- Export: one confirmation dialog, edit just the filename, saved into the folder.
  **The filename extension selects the archive format** (`.tar`, `.tar.gz`/`.tgz`,
  `.tar.xz`/`.txz`, `.vhdx`/`.vhd`).
- Import: pick an archive from a list; type only the new distro name; the install
  directory is chosen automatically. Format is auto-handled by extension.
- Allow importing an archive from outside the managed folder (custom path).
- Allow deleting old archives from the import picker (housekeeping).
- Default export filename uses **local** time.

**Non-goals (v1, YAGNI)**
- A catalog/metadata file (source distro, timestamps) — the filesystem is the index.
- Auto-removing `installed\<name>\` when a distro is unregistered.
- Type-to-filter inside the import picker (lists are short; can be added later).
- Managing the install location of distros **not** created through `wslm`.
- A timezone-database crate (`chrono`/`time`); local time comes from a single Win32
  call (see §6).

## 3. Managed folder layout & configuration

```
<manage_dir>\                         default: %USERPROFILE%\wsl-manager
  exports\                            export archives (tar / tar.gz / tar.xz / vhdx)
    Ubuntu-20260607-153012.tar
    Debian-20260605-090145.tar.gz
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
- `export_filename(distro: &str, local_ts: &str) -> String`
  → `"<distro>-<local_ts>.tar"` (default extension `.tar`; `local_ts` is
  `YYYYMMDD-HHMMSS`, see §6)
- `derive_distro_name(archive_filename: &str) -> String` → filename with the archive
  extension stripped (`.tar`, `.tar.gz`, `.tgz`, `.tar.xz`, `.txz`, `.vhdx`, `.vhd`)
- `sanitize_name(name: &str) -> String` → strip path separators / illegal chars
- **`ExportFormat`** enum `{ Tar, TarGz, TarXz, Vhd }`:
  - `from_filename(name: &str) -> ExportFormat` (by extension; default `Tar`)
  - `wsl_format_arg(self) -> Option<&'static str>` → `None` for `Tar` (omit
    `--format` for maximum back-compat), else `Some("tar.gz"|"tar.xz"|"vhd")`
- **`is_vhd_archive(name: &str) -> bool`** → ends with `.vhd`/`.vhdx`
  (drives the import `--vhd` flag)

Filesystem helpers (synchronous; the runtime calls these via
`tokio::task::spawn_blocking`, mirroring `config::load_wslconfig`):
- `list_exports(root: &Path) -> io::Result<Vec<Archive>>` — files in `exports\` whose
  extension is a recognised archive type, sorted newest-first by mtime; missing dir
  ⇒ `Ok(vec![])`
- `delete_export(path: &Path) -> io::Result<()>`
- `ensure_dirs(root: &Path) -> io::Result<()>` — create `exports\` (and `installed\`)

```rust
pub struct Archive {
    pub name: String,     // file name, e.g. "Ubuntu-20260607-153012.tar.gz"
    pub path: PathBuf,    // absolute path
    pub size: u64,        // bytes (shown via ui::util::human_size)
}
```
(Modified-time is used only for sorting; it is not displayed in v1, so no date
formatting is needed for the listing.)

### 4.2 MVU additions (`src/app/`)

New `Command` variants (`message.rs`):
- `OpenExportDialog { distro: String }` — runtime computes the local-time default
  filename and ensures `exports\` exists.
- `ListExports` — runtime lists `exports\`.
- `DeleteExport(PathBuf)` — runtime deletes an archive, then re-lists.

Changed `Command`s (carry the format, derived purely from the filename):
- `Export { name, path, format: ExportFormat }`
- `Import { name, dir, tar, vhd: bool }`

New `Action` variants:
- `ExportDialogReady { distro: String, filename: String }`
- `ExportsListed(Vec<Archive>)`

New `Modal` variant: `Modal::ImportPick(ImportPickState)`.

```rust
pub struct ImportPickState { pub entries: Vec<Archive>, pub selected: usize }
// select_next / select_prev (clamped), selected_entry() -> Option<&Archive>
```

New `FormKind`s (`modal.rs`):
- `FormKind::ImportName { tar: PathBuf }` — one field: new distro name.
- `FormKind::ImportCustom` — two fields: archive path, new distro name.

Export keeps `FormKind::Export { distro }` but its single field now means **filename**
(not a full path); the reducer builds the command via
`exports_dir(root).join(filename)` and `ExportFormat::from_filename(filename)`.

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
2. Runtime: `ensure_dirs`, build `filename = export_filename(distro, local_ts)` (see
   §6) → `Action::ExportDialogReady { distro, filename }`.
3. Reducer opens `Modal::Form(Export)` with the filename field pre-filled; the UI
   shows the fixed `→ <manage_dir>\exports\` prefix and a hint that the **extension
   chooses the format** (`.tar` / `.tar.gz` / `.tar.xz` / `.vhdx`).
4. Enter (non-empty) → `Modal::Progress` + `Command::Export { name: distro,
   path: exports_dir(root).join(filename), format: ExportFormat::from_filename(...) }`.
   Esc cancels.
5. Backend runs `wsl --export <name> <path>` plus `--format <fmt>` when
   `wsl_format_arg` is `Some` (omitted for plain `tar`).
6. `OpDone`/`OpFailed` as today. No overwrite check (the local timestamp makes
   collisions effectively impossible; editing to an existing name overwrites, which
   matches `wsl --export` behaviour).

### 5.2 Import (`m`)
1. `m` → `Command::ListExports` → `Action::ExportsListed` → `Modal::ImportPick`.
2. Picker keys:
   - `↑`/`↓` (and `j`/`k` per `keybind_style`) — move selection
   - `Enter` — choose the selected archive → open `Modal::Form(ImportName { tar })`
     with the name field pre-filled from `derive_distro_name`
   - `c` — custom archive: open `Modal::Form(ImportCustom)` (archive path + name)
   - `d` — delete the selected archive → `Modal::Confirm` → `Command::DeleteExport`
   - `Esc` — close the picker
   Footer hint lists these. Empty list shows an "(empty — press `c`)" message; only
   `c`/`Esc` are actionable.
3. Name form submit (`ImportName`/`ImportCustom`):
   - Validate required fields non-empty (else keep the form open, as today).
   - If a registered distro already has that name (case-insensitive, checked against
     `model.distros` — pure), show the existing `PromptImportOverwrite` confirm.
   - Otherwise → `Modal::Progress` + `Command::Import { name, dir:
     installed_dir(root, name), tar, vhd: is_vhd_archive(tar) }`.
4. Runtime `create_dir_all(dir)` before `backend.import`. Backend runs
   `wsl --import <name> <dir> <tar>`, adding `--vhd` when `vhd` is true; tar/tar.gz/
   tar.xz need no flag (WSL auto-detects compression). `OpDone`/`OpFailed` as today.

### 5.3 Delete (`d` in the picker)
`d` → `Modal::Confirm` (prompt names the file) → `Command::DeleteExport(path)`.
On success the runtime re-runs `ListExports` and re-emits `ExportsListed`, so the
picker reopens refreshed (selection clamped); a status message reports the deletion.
On failure → `OpFailed` modal.

## 6. Local timestamp (design decision)

`export_filename` needs a timestamp at dialog-open time. The reducer is pure (no
clock), so the **runtime** supplies it: `OpenExportDialog` → runtime reads the local
time → `ExportDialogReady`. This mirrors the existing async-open patterns
(`ConfigLoaded`, `OnlineList`).

Local time is read with the Win32 **`GetLocalTime`** API (via `windows-sys`, feature
`Win32_System_SystemInformation`), which returns a `SYSTEMTIME` already broken down
into local year/month/day/hour/minute/second — no timezone math and no timezone-DB
crate. The runtime formats it as `YYYYMMDD-HHMMSS` and passes the string to the pure
`manage::export_filename`, which stays deterministic and unit-testable. This suits
the Windows-only project (which already depends on the `windows` crate family via
`windows-registry`).

Alternative considered: `chrono`/`time` with local offset — rejected to avoid a
larger dependency, and because `time`'s local-offset is unreliable in multithreaded
(tokio) programs.

## 7. Archive format support (WSL capability note)

Verified against `wsl.exe` 2.7.3 on the target machine (the Microsoft Learn pages
currently document only `--vhd`, which is stale):

- `wsl --export <Distro> <File> [--format <tar|tar.gz|tar.xz|vhd>]` — defaults to
  `tar`. We pass `--format` only for non-tar formats (back-compat for plain tar).
- `wsl --import <Distro> <Dir> <File> [--vhd]` — imports tar; `--vhd` for `.vhd(x)`.
  Gzip/xz-compressed tar is auto-detected on import, so no flag is needed for
  `.tar.gz`/`.tar.xz`.

The extension typed by the user (export) or chosen from the list (import) is the
single source of truth for the format; `ExportFormat::from_filename` /
`is_vhd_archive` map it to the right `wsl` arguments. If a very old `wsl` rejects
`--format`, the export fails into the normal `OpFailed` modal (graceful).

## 8. i18n

Add `Key`s (en/ja both filled; keep `Key::ALL` in sync — enforced by
`every_key_has_both_languages`):
- `FormImportNameTitle`, `LabelImportNameOnly`, `LabelImportCustomArchive`
- `PickImportTitle`, `PickImportEmpty`, `PickImportHints` (Enter/​c/​d/​Esc)
- `ExportSavedToHint` (the `→ exports\` prefix) and `ExportFormatHint`
  ("extension selects format: .tar/.tar.gz/.tar.xz/.vhdx")
- `PromptDeleteArchive`, `DoneDeletedArchive`, `StatusListingExports`

Existing keys reused: `ProgExporting`, `ProgImporting`, `PromptImportOverwrite`,
`DoneExported`, `DoneImported`, `FormExportTitle`, `LabelExportPath` (relabelled to
"Output file name" / "出力ファイル名").

## 9. Error handling

- All `manage` fs errors surface through the existing `OpFailed` → `Modal::Error`.
- A missing/empty `exports\` is not an error: `list_exports` returns empty and the
  picker shows the empty hint.
- An unwritable `manage_dir` (mkdir/export/import failure) or an unsupported
  `--format`/`--vhd` → `OpFailed` with the OS / `wsl` error message (localized
  wrapper `Key::FailOp`).

## 10. Testing

- **`manage` (pure):** `exports_dir`/`installed_dir`; `export_filename` (fixed
  `local_ts` → exact string); `derive_distro_name` across all extensions;
  `ExportFormat::from_filename` + `wsl_format_arg`; `is_vhd_archive`; `sanitize_name`.
- **backend (pure-ish):** the exact `wsl` arg vectors built for each format
  (`--format` omitted for tar, present for gz/xz/vhd; `--vhd` on import for vhd(x)).
- **Reducer:** `e` → `OpenExportDialog`; `ExportDialogReady` opens the export form;
  export submit builds the correct `exports\…` path **and** `ExportFormat`; `m` →
  `ListExports`; `ExportsListed` opens `ImportPick`; picker nav / `c` / `d`-confirm /
  `Enter` → name form; name submit → `Import { dir = installed\<name>\, tar, vhd }`;
  name collision → overwrite confirm; `DeleteExport` re-lists.
- **prefs:** TOML round-trip incl. `manage_dir`; default resolves when `None`.
- **UI (TestBackend):** `ImportPick` render (list + size + hints + empty state),
  export form (filename + prefix + format hint), import name/custom forms.
- `manage::list_exports`/`delete_export` against a `tempdir` may be `#[ignore]`d or
  use a temp path; pure helpers are always tested.

## 11. File-by-file change summary

- `Cargo.toml` — add `windows-sys` (feature `Win32_System_SystemInformation`) for
  `GetLocalTime`.
- `src/prefs/mod.rs` — add `manage_dir` field + `manage_dir()` resolver + test.
- `src/manage/mod.rs` — **new**: pure helpers (`ExportFormat`, `is_vhd_archive`,
  paths, name/filename) + fs helpers + `Archive` + tests.
- `src/lib.rs` — register `manage` module (IO-independent layer).
- `src/app/model.rs` — add `manage_dir: PathBuf` to `Model`.
- `src/app/message.rs` — `Command::{OpenExportDialog, ListExports, DeleteExport}`;
  `Export`/`Import` gain `format`/`vhd`; `Action::{ExportDialogReady, ExportsListed}`.
- `src/app/modal.rs` — `Modal::ImportPick`, `ImportPickState`; `FormKind::{ImportName,
  ImportCustom}`; remove `FormKind::Import`; `FormState` constructors.
- `src/app/update/mod.rs` — `e`/`m` handlers; new actions; export form open via
  `ExportDialogReady`.
- `src/app/update/modal.rs` — `handle_import_pick_key`; export/import submit paths
  (build `ExportFormat` / `vhd`).
- `src/wsl/backend.rs` — `export(name, path, format)` and `import(name, dir, tar, vhd)`
  build the `--format`/`--vhd` args; update the `WslBackend` trait + `RealWslBackend`
  + the test `MockBackend`.
- `src/runtime/mod.rs` — set `model.manage_dir`; `local` timestamp via `GetLocalTime`;
  dispatch `OpenExportDialog`, `ListExports`, `DeleteExport`; `create_dir_all(dir)`
  before `Import`.
- `src/ui/mod.rs` — render `ImportPick`; export filename prefix + format hint; import
  forms.
- `src/i18n/mod.rs` — new keys (en/ja) + `Key::ALL`.

## 12. Keybindings (summary)

- List: `e` export (unchanged), `m` import (now opens the picker).
- Import picker: `↑`/`↓`(+`j`/`k`) move · `Enter` import · `c` custom archive ·
  `d` delete · `Esc` back.
- Export dialog / import name dialog: edit field · `Enter` confirm · `Esc` cancel.
  Export: the filename **extension** chooses the format.
