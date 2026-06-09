# Managed export/import folder — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a configurable managed folder that owns export archives (`exports\`) and the vhdx of wslm-imported distros (`installed\<name>\`), so export/import is selection-based with minimal typing, with the filename extension choosing the archive format (tar/tar.gz/tar.xz/vhd) and local-time default names.

**Architecture:** Keep the lib's layering — a new pure-/fs-only `manage` module holds path/format/name logic; the pure reducer builds `Command`s from `model.manage_dir`; the runtime performs all IO (listing, delete, mkdir, local clock, `wsl` args). A new `Modal::ImportPick` reuses the existing picker/form/confirm patterns.

**Tech Stack:** Rust 1.96, ratatui 0.30, crossterm 0.29, tokio, `windows-sys` (new, for `GetLocalTime`).

Reference spec: `docs/superpowers/specs/2026-06-07-managed-export-import-folder-design.md`

---

## File structure

- `Cargo.toml` — add `windows-sys` (GetLocalTime).
- `src/manage/mod.rs` — **new**. Pure: `exports_dir`, `installed_dir`, `export_filename`, `derive_distro_name`, `sanitize_name`, `ExportFormat`, `is_vhd_archive`, `is_archive`. FS: `Archive`, `list_exports`, `delete_export`, `ensure_dirs`.
- `src/lib.rs` — register `pub mod manage;`.
- `src/prefs/mod.rs` — `manage_dir: Option<PathBuf>` + `manage_dir()`.
- `src/app/model.rs` — `manage_dir: PathBuf` field.
- `src/app/message.rs` — new `Command`/`Action`; `Export`/`Import` gain `format`/`vhd`.
- `src/app/modal.rs` — `Modal::ImportPick(ImportPickState)`; `FormKind::{ImportName, ImportCustom}`; remove `FormKind::Import`; `FormState::{import_name, import_custom}`.
- `src/app/update/mod.rs` — `e`/`m` handlers; `ExportDialogReady`/`ExportsListed`.
- `src/app/update/modal.rs` — `handle_import_pick_key`; export/import submit.
- `src/wsl/backend.rs` — `export(.., format)` / `import(.., vhd)`.
- `src/runtime/mod.rs` — `model.manage_dir`; `local_timestamp()`; dispatch new commands; `create_dir_all` before export/import.
- `src/i18n/mod.rs` — new keys (en/ja) + `Key::ALL`.
- `src/ui/mod.rs` — render `ImportPick`; export form hint; import forms.

Build/lint/test gates after each task: `cargo build`, `cargo clippy --all-targets -- -D warnings`, `cargo test --all`, `cargo fmt --all -- --check` (Windows host).

---

## Task 1: prefs `manage_dir`

**Files:**
- Modify: `src/prefs/mod.rs`

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `src/prefs/mod.rs`:

```rust
    #[test]
    fn manage_dir_defaults_under_userprofile() {
        let prefs = Prefs::default();
        let dir = prefs.manage_dir();
        assert!(dir.ends_with("wsl-manager"), "got {dir:?}");
    }

    #[test]
    fn manage_dir_uses_configured_path() {
        let prefs = Prefs {
            manage_dir: Some(std::path::PathBuf::from(r"D:\wsl")),
            ..Default::default()
        };
        assert_eq!(prefs.manage_dir(), std::path::PathBuf::from(r"D:\wsl"));
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib prefs::tests::manage_dir -v`
Expected: FAIL — no field `manage_dir` / no method `manage_dir`.

- [ ] **Step 3: Add the field, default, and resolver**

In `struct Prefs` add the field (after `default_shell_launch`):

```rust
    /// Root folder for managed export archives and imported-distro vhdx.
    /// `None` means the default (`%USERPROFILE%\wsl-manager`).
    pub manage_dir: Option<PathBuf>,
```

In `impl Default for Prefs` add `manage_dir: None,`.

Add to `impl Prefs`:

```rust
    /// The effective managed-folder root (configured path, else
    /// `%USERPROFILE%\wsl-manager`).
    pub fn manage_dir(&self) -> PathBuf {
        if let Some(dir) = &self.manage_dir {
            return dir.clone();
        }
        let base = std::env::var_os("USERPROFILE")
            .map(PathBuf::from)
            .unwrap_or_default();
        base.join("wsl-manager")
    }
```

Also extend `round_trips_through_toml`'s constructed `Prefs` with `manage_dir: Some(PathBuf::from(r"C:\wsl"))` so the round-trip covers it.

- [ ] **Step 4: Run tests**

Run: `cargo test --lib prefs:: -v`
Expected: PASS (all prefs tests).

- [ ] **Step 5: Commit**

```bash
git add src/prefs/mod.rs
git commit -m "feat(prefs): add manage_dir setting (default %USERPROFILE%\\wsl-manager)"
```

---

## Task 2: `manage` module — pure helpers

**Files:**
- Create: `src/manage/mod.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Create the module with pure helpers**

Create `src/manage/mod.rs`:

```rust
//! Managed export/import folder: path, archive-format and name helpers (pure),
//! plus a thin filesystem layer (listing/deleting archives). Terminal-/clock-
//! independent so it can be unit-tested headlessly; the runtime performs the IO.

use std::path::{Path, PathBuf};

/// The export archive formats `wsl --export` can produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    /// Uncompressed tar (the `wsl` default; pass no `--format`).
    Tar,
    /// gzip-compressed tar (`--format tar.gz`).
    TarGz,
    /// xz-compressed tar (`--format tar.xz`).
    TarXz,
    /// Virtual hard disk (`--format vhd`).
    Vhd,
}

impl ExportFormat {
    /// Pick the format from a filename's extension (defaults to `Tar`).
    pub fn from_filename(name: &str) -> ExportFormat {
        let lower = name.to_ascii_lowercase();
        if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
            ExportFormat::TarGz
        } else if lower.ends_with(".tar.xz") || lower.ends_with(".txz") {
            ExportFormat::TarXz
        } else if lower.ends_with(".vhdx") || lower.ends_with(".vhd") {
            ExportFormat::Vhd
        } else {
            ExportFormat::Tar
        }
    }

    /// The `--format` value, or `None` for plain tar (omit the flag for
    /// back-compat with older `wsl`).
    pub fn wsl_format_arg(self) -> Option<&'static str> {
        match self {
            ExportFormat::Tar => None,
            ExportFormat::TarGz => Some("tar.gz"),
            ExportFormat::TarXz => Some("tar.xz"),
            ExportFormat::Vhd => Some("vhd"),
        }
    }
}

/// Whether a filename is a VHD archive (import needs `--vhd` for these).
pub fn is_vhd_archive(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".vhdx") || lower.ends_with(".vhd")
}

/// Recognised archive extensions, longest-first so `.tar.gz` wins over `.tar`.
const ARCHIVE_EXTS: [&str; 7] = [
    ".tar.gz", ".tar.xz", ".tgz", ".txz", ".vhdx", ".tar", ".vhd",
];

/// Whether a filename has a recognised archive extension.
pub fn is_archive(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    ARCHIVE_EXTS.iter().any(|ext| lower.ends_with(ext))
}

/// The `exports\` subfolder of the managed root.
pub fn exports_dir(root: &Path) -> PathBuf {
    root.join("exports")
}

/// The `installed\<name>\` folder for an imported distro.
pub fn installed_dir(root: &Path, name: &str) -> PathBuf {
    root.join("installed").join(sanitize_name(name))
}

/// Replace characters illegal in Windows file names with `_`.
pub fn sanitize_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect::<String>()
        .trim()
        .to_string()
}

/// The default export filename: `<distro>-<local_ts>.tar`. `local_ts` is the
/// runtime-supplied local timestamp (`YYYYMMDD-HHMMSS`).
pub fn export_filename(distro: &str, local_ts: &str) -> String {
    format!("{}-{}.tar", sanitize_name(distro), local_ts)
}

/// A distro-name default derived from an archive filename (extension stripped).
pub fn derive_distro_name(archive_filename: &str) -> String {
    let lower = archive_filename.to_ascii_lowercase();
    for ext in ARCHIVE_EXTS {
        if lower.ends_with(ext) {
            return archive_filename[..archive_filename.len() - ext.len()].to_string();
        }
    }
    archive_filename.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_format_from_extension() {
        assert_eq!(ExportFormat::from_filename("a.tar"), ExportFormat::Tar);
        assert_eq!(ExportFormat::from_filename("a.TAR.GZ"), ExportFormat::TarGz);
        assert_eq!(ExportFormat::from_filename("a.tgz"), ExportFormat::TarGz);
        assert_eq!(ExportFormat::from_filename("a.tar.xz"), ExportFormat::TarXz);
        assert_eq!(ExportFormat::from_filename("a.vhdx"), ExportFormat::Vhd);
        assert_eq!(ExportFormat::from_filename("a.bak"), ExportFormat::Tar);
    }

    #[test]
    fn export_format_wsl_arg() {
        assert_eq!(ExportFormat::Tar.wsl_format_arg(), None);
        assert_eq!(ExportFormat::TarGz.wsl_format_arg(), Some("tar.gz"));
        assert_eq!(ExportFormat::Vhd.wsl_format_arg(), Some("vhd"));
    }

    #[test]
    fn vhd_detection() {
        assert!(is_vhd_archive("x.vhdx"));
        assert!(is_vhd_archive("X.VHD"));
        assert!(!is_vhd_archive("x.tar.gz"));
    }

    #[test]
    fn paths_compose() {
        let root = Path::new(r"C:\wsl");
        assert_eq!(exports_dir(root), Path::new(r"C:\wsl\exports"));
        assert_eq!(installed_dir(root, "My/Distro"), Path::new(r"C:\wsl\installed\My_Distro"));
    }

    #[test]
    fn export_filename_uses_timestamp() {
        assert_eq!(export_filename("Ubuntu", "20260607-153012"), "Ubuntu-20260607-153012.tar");
    }

    #[test]
    fn derive_name_strips_archive_extension() {
        assert_eq!(derive_distro_name("Ubuntu-20260607.tar.gz"), "Ubuntu-20260607");
        assert_eq!(derive_distro_name("Debian.tar"), "Debian");
        assert_eq!(derive_distro_name("box.vhdx"), "box");
        assert_eq!(derive_distro_name("noext"), "noext");
    }

    #[test]
    fn sanitize_replaces_illegal_chars() {
        assert_eq!(sanitize_name("a:b/c"), "a_b_c");
    }
}
```

- [ ] **Step 2: Register the module**

In `src/lib.rs`, add `pub mod manage;` next to the other IO-independent layer modules (alphabetical with `config`, `metrics`, etc.). If the module doc lists the terminal-independent layers, add `manage` to that list.

- [ ] **Step 3: Run the tests (verify they pass)**

Run: `cargo test --lib manage::tests -v`
Expected: PASS (7 tests).

- [ ] **Step 4: Commit**

```bash
git add src/manage/mod.rs src/lib.rs
git commit -m "feat(manage): pure path/format/name helpers for the managed folder"
```

---

## Task 3: `manage` module — filesystem layer

**Files:**
- Modify: `src/manage/mod.rs`

- [ ] **Step 1: Write the failing test**

Append to `src/manage/mod.rs` `tests` module:

```rust
    use std::sync::atomic::{AtomicU32, Ordering};

    static TMP_SEQ: AtomicU32 = AtomicU32::new(0);

    /// A unique temp directory for an fs test (cleaned up by the test).
    fn temp_root() -> PathBuf {
        let n = TMP_SEQ.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("wslm-test-{}-{}", std::process::id(), n));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn list_exports_filters_and_sorts() {
        let root = temp_root();
        let exp = exports_dir(&root);
        std::fs::create_dir_all(&exp).unwrap();
        std::fs::write(exp.join("a.tar"), b"1").unwrap();
        std::fs::write(exp.join("b.tar.gz"), b"22").unwrap();
        std::fs::write(exp.join("notes.txt"), b"x").unwrap(); // ignored
        let names: Vec<String> = list_exports(&root).unwrap().into_iter().map(|a| a.name).collect();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"a.tar".to_string()));
        assert!(names.contains(&"b.tar.gz".to_string()));
        assert!(!names.iter().any(|n| n == "notes.txt"));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn list_exports_missing_dir_is_empty() {
        let root = temp_root();
        assert!(list_exports(&root).unwrap().is_empty());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn ensure_dirs_then_delete() {
        let root = temp_root();
        ensure_dirs(&root).unwrap();
        assert!(exports_dir(&root).is_dir());
        assert!(root.join("installed").is_dir());
        let f = exports_dir(&root).join("x.tar");
        std::fs::write(&f, b"1").unwrap();
        delete_export(&f).unwrap();
        assert!(!f.exists());
        std::fs::remove_dir_all(&root).ok();
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib manage::tests::list_exports -v`
Expected: FAIL — `list_exports`/`ensure_dirs`/`delete_export`/`Archive` not found.

- [ ] **Step 3: Add the filesystem layer**

Add to `src/manage/mod.rs` (after the pure helpers, before `#[cfg(test)]`):

```rust
use std::io;

/// An export archive on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Archive {
    /// File name (e.g. `Ubuntu-20260607-153012.tar.gz`).
    pub name: String,
    /// Absolute path.
    pub path: PathBuf,
    /// Size in bytes.
    pub size: u64,
}

/// List recognised archives in `<root>\exports`, newest-first. A missing folder
/// yields an empty list.
pub fn list_exports(root: &Path) -> io::Result<Vec<Archive>> {
    let dir = exports_dir(root);
    let read = match std::fs::read_dir(&dir) {
        Ok(read) => read,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut rows: Vec<(Option<std::time::SystemTime>, Archive)> = Vec::new();
    for entry in read.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !is_archive(name) {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        if !meta.is_file() {
            continue;
        }
        rows.push((
            meta.modified().ok(),
            Archive {
                name: name.to_string(),
                path: path.clone(),
                size: meta.len(),
            },
        ));
    }
    rows.sort_by(|a, b| b.0.cmp(&a.0)); // newest first
    Ok(rows.into_iter().map(|(_, a)| a).collect())
}

/// Delete an archive file.
pub fn delete_export(path: &Path) -> io::Result<()> {
    std::fs::remove_file(path)
}

/// Create `<root>\exports` and `<root>\installed` if missing.
pub fn ensure_dirs(root: &Path) -> io::Result<()> {
    std::fs::create_dir_all(exports_dir(root))?;
    std::fs::create_dir_all(root.join("installed"))?;
    Ok(())
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test --lib manage::tests -v`
Expected: PASS (all manage tests).

- [ ] **Step 5: Commit**

```bash
git add src/manage/mod.rs
git commit -m "feat(manage): list/delete/ensure filesystem helpers"
```

---

## Task 4: backend — format/vhd-aware export/import

**Files:**
- Modify: `src/wsl/backend.rs`
- Modify: `src/runtime/mod.rs` (the `MockBackend` in `#[cfg(test)]`)

- [ ] **Step 1: Update the trait + real impl**

In `src/wsl/backend.rs`, add the import at the top:

```rust
use crate::manage::ExportFormat;
```

Change the trait methods:

```rust
    /// Export a distro (`wsl --export <name> <path> [--format <fmt>]`).
    async fn export(&self, name: &str, path: &Path, format: ExportFormat) -> Result<()>;
    /// Import a distro (`wsl --import <name> <dir> <tar> [--vhd]`).
    async fn import(&self, name: &str, dir: &Path, tar: &Path, vhd: bool) -> Result<()>;
```

Replace the `RealWslBackend` impls:

```rust
    async fn export(&self, name: &str, path: &Path, format: ExportFormat) -> Result<()> {
        let path = path.to_string_lossy();
        let mut args = vec!["--export", name, path.as_ref()];
        if let Some(fmt) = format.wsl_format_arg() {
            args.push("--format");
            args.push(fmt);
        }
        run_wsl_long(&args).await.map(drop)
    }

    async fn import(&self, name: &str, dir: &Path, tar: &Path, vhd: bool) -> Result<()> {
        let dir = dir.to_string_lossy();
        let tar = tar.to_string_lossy();
        let mut args = vec!["--import", name, dir.as_ref(), tar.as_ref()];
        if vhd {
            args.push("--vhd");
        }
        run_wsl_long(&args).await.map(drop)
    }
```

- [ ] **Step 2: Update the test `MockBackend`**

In `src/runtime/mod.rs` `#[cfg(test)] mod tests`, change the `MockBackend` impls to match the new signatures (record the format/vhd so it stays useful):

```rust
        async fn export(&self, name: &str, _path: &Path, format: crate::manage::ExportFormat) -> Result<()> {
            self.record(format!("export {name} {format:?}"))
        }
        async fn import(&self, name: &str, _dir: &Path, _tar: &Path, vhd: bool) -> Result<()> {
            self.record(format!("import {name} vhd={vhd}"))
        }
```

(If any other `WslBackend` impl exists, update it too — run `rg "impl .*WslBackend"` to be sure.)

- [ ] **Step 3: Build**

Run: `cargo build`
Expected: compiles (callers of `export`/`import` in `runtime` will be updated in Task 9; if they error now, that's expected — proceed to Step 4 only after Task 9, or temporarily this task may leave `runtime` callers broken). To keep the build green, do Task 9's runtime wiring before re-running the full build; for now verify just the library type-checks the backend with `cargo build 2>&1 | rg "backend.rs"` showing no errors in `backend.rs`.

> Note: backend signature changes ripple into `message.rs`/`runtime`. The next tasks complete the wiring; the first fully-green build is at the end of Task 9.

- [ ] **Step 4: Commit**

```bash
git add src/wsl/backend.rs src/runtime/mod.rs
git commit -m "feat(wsl): export takes ExportFormat, import takes vhd flag"
```

---

## Task 5: message types — new commands/actions, format/vhd payloads

**Files:**
- Modify: `src/app/message.rs`

- [ ] **Step 1: Add imports and change `Command`**

In `src/app/message.rs` add to the `use` block:

```rust
use crate::manage::{Archive, ExportFormat};
```

Change `Command::Export` and `Command::Import`:

```rust
    /// Export a distro to an archive file.
    Export {
        /// The distro to export.
        name: String,
        /// Destination archive path (under the managed `exports\`).
        path: PathBuf,
        /// Archive format (derived from the filename extension).
        format: ExportFormat,
    },
    /// Import a distro from an archive file.
    Import {
        /// New distro name.
        name: String,
        /// Install directory (managed `installed\<name>\`).
        dir: PathBuf,
        /// Source archive path.
        tar: PathBuf,
        /// Whether the source is a `.vhd(x)` (adds `--vhd`).
        vhd: bool,
    },
```

Add these new variants to `Command`:

```rust
    /// Build the timestamped default export filename and open the export dialog.
    OpenExportDialog {
        /// The distro to export.
        distro: String,
    },
    /// List archives in the managed `exports\` folder.
    ListExports,
    /// Delete a managed archive, then re-list.
    DeleteExport(PathBuf),
```

- [ ] **Step 2: Add the new `Action` variants**

In `enum Action` add:

```rust
    /// The runtime computed the default export filename; open the export form.
    ExportDialogReady {
        /// The distro to export.
        distro: String,
        /// The default (editable) filename.
        filename: String,
    },
    /// The managed `exports\` listing arrived.
    ExportsListed(Vec<Archive>),
```

- [ ] **Step 3: Build the crate's type layer**

Run: `cargo build 2>&1 | rg "message.rs"`
Expected: no errors reported in `message.rs` (downstream match arms are completed in later tasks).

- [ ] **Step 4: Commit**

```bash
git add src/app/message.rs
git commit -m "feat(app): managed export/import commands and actions"
```

---

## Task 6: model — `manage_dir`

**Files:**
- Modify: `src/app/model.rs`

- [ ] **Step 1: Add the field**

In `src/app/model.rs`, ensure `use std::path::PathBuf;` is present (add it next to the `HashSet` import). Add to `struct Model` (e.g. after `loaded`):

```rust
    /// Root of the managed export/import folder (resolved from prefs at startup).
    pub manage_dir: PathBuf,
```

`Model` derives `Default`, so `PathBuf::default()` (empty) is the test default; the runtime sets the real value in Task 9.

- [ ] **Step 2: Build**

Run: `cargo build 2>&1 | rg "model.rs"`
Expected: no errors in `model.rs`.

- [ ] **Step 3: Commit**

```bash
git add src/app/model.rs
git commit -m "feat(app): Model.manage_dir for managed-folder paths"
```

---

## Task 7: modal types — ImportPick, new FormKinds

**Files:**
- Modify: `src/app/modal.rs`

- [ ] **Step 1: Add the import and the `ImportPick` modal variant**

In `src/app/modal.rs` add:

```rust
use crate::manage::Archive;
```

Add to `enum Modal`:

```rust
    /// A picker of managed export archives to import.
    ImportPick(ImportPickState),
```

- [ ] **Step 2: Replace `FormKind` and add `FormState` constructors**

Change `enum FormKind`:

```rust
pub enum FormKind {
    /// Export the named distro (one field: the output filename).
    Export {
        /// The distro being exported.
        distro: String,
    },
    /// Import the chosen archive (one field: the new distro name).
    ImportName {
        /// The selected archive path.
        tar: std::path::PathBuf,
    },
    /// Import a custom archive (fields: archive path, new distro name).
    ImportCustom,
}
```

Replace `FormState::import` with:

```rust
    /// An import form for a picked archive: just the new distro name.
    pub fn import_name(tar: std::path::PathBuf, default_name: String) -> Self {
        Self {
            kind: FormKind::ImportName { tar },
            labels: vec![Key::LabelImportNameOnly],
            fields: vec![TextField::new(default_name)],
            focus: 0,
        }
    }

    /// An import form for a custom archive path plus the new distro name.
    pub fn import_custom() -> Self {
        Self {
            kind: FormKind::ImportCustom,
            labels: vec![Key::LabelImportCustomArchive, Key::LabelImportNameOnly],
            fields: vec![TextField::default(), TextField::default()],
            focus: 0,
        }
    }
```

(The `export` constructor is unchanged; its single `LabelExportPath` field is relabelled to "Output file name" in Task 11's i18n step.)

- [ ] **Step 3: Add `ImportPickState`**

Add near `InstallPickState`:

```rust
/// State for the managed-archive import picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportPickState {
    /// Archives found in the managed `exports\` folder (newest-first).
    pub entries: Vec<Archive>,
    /// Selected index.
    pub selected: usize,
}

impl ImportPickState {
    /// A picker over `entries`.
    pub fn new(entries: Vec<Archive>) -> Self {
        Self { entries, selected: 0 }
    }

    /// Move selection down (clamped).
    pub fn select_next(&mut self) {
        if !self.entries.is_empty() {
            self.selected = (self.selected + 1).min(self.entries.len() - 1);
        }
    }

    /// Move selection up (clamped).
    pub fn select_prev(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    /// The selected archive, if any.
    pub fn selected_entry(&self) -> Option<&Archive> {
        self.entries.get(self.selected)
    }
}
```

- [ ] **Step 4: Re-export from `app` if needed**

If `src/app/mod.rs` re-exports modal types (e.g. `pub use modal::{... InstallPickState ...}`), add `ImportPickState` to that list so `crate::app::ImportPickState` resolves (used by `ui`).

- [ ] **Step 5: Build the type layer**

Run: `cargo build 2>&1 | rg "modal.rs"`
Expected: no errors in `modal.rs` (the `i18n` keys `LabelImportNameOnly`/`LabelImportCustomArchive` are added in Task 11; until then this references not-yet-existing keys. Add Task 11's i18n keys before the first green build, or expect this key reference to error until then.)

- [ ] **Step 6: Commit**

```bash
git add src/app/modal.rs src/app/mod.rs
git commit -m "feat(app): ImportPick modal + ImportName/ImportCustom form kinds"
```

---

## Task 8: i18n — new keys

**Files:**
- Modify: `src/i18n/mod.rs`

- [ ] **Step 1: Add the key variants**

In `enum Key` add (near the export/import keys):

```rust
    LabelImportNameOnly,
    LabelImportCustomArchive,
    PickImportTitle,
    PickImportEmpty,
    PickImportHints,
    ExportFormatHint,
    PromptDeleteArchive,
    DoneDeletedArchive,
```

- [ ] **Step 2: Add them to `Key::ALL`**

Add the same eight variants to the `Key::ALL` array.

- [ ] **Step 3: Add the `entry()` arms (en, ja) and relabel export path**

In `entry()` add:

```rust
        Key::LabelImportNameOnly => ("New distro name", "新しいディストロ名"),
        Key::LabelImportCustomArchive => ("Source archive path", "元アーカイブのパス"),
        Key::PickImportTitle => (" Import — pick an archive ", " インポート — アーカイブを選択 "),
        Key::PickImportEmpty => ("(no archives in exports\\ — press c for a custom path)",
                                 "(exports\\ にアーカイブがありません — c で任意パス)"),
        Key::PickImportHints => ("↑/↓ move · Enter import · c custom · d delete · Esc back",
                                 "↑/↓ 移動 · Enter 取込 · c 任意 · d 削除 · Esc 戻る"),
        Key::ExportFormatHint => ("Saved under exports\\; extension picks format (.tar/.tar.gz/.tar.xz/.vhdx)",
                                  "exports\\ に保存。拡張子で形式選択 (.tar/.tar.gz/.tar.xz/.vhdx)"),
        Key::PromptDeleteArchive => ("Delete archive '{}'?", "アーカイブ '{}' を削除しますか？"),
        Key::DoneDeletedArchive => ("Deleted '{}'", "'{}' を削除しました"),
```

Relabel the existing export-path key:

```rust
        Key::LabelExportPath => ("Output file name", "出力ファイル名"),
```

- [ ] **Step 4: Run the i18n completeness test**

Run: `cargo test --lib i18n -v`
Expected: PASS — `every_key_has_both_languages` covers the new keys.

- [ ] **Step 5: Commit**

```bash
git add src/i18n/mod.rs
git commit -m "feat(i18n): keys for the import picker, format hint, delete"
```

---

## Task 9: runtime — clock, dispatch, mkdir, model.manage_dir

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/runtime/mod.rs`

- [ ] **Step 1: Add `windows-sys`**

In `Cargo.toml` `[dependencies]` add:

```toml
windows-sys = { version = "0.59", features = ["Win32_Foundation", "Win32_System_SystemInformation"] }
```

- [ ] **Step 2: Set `model.manage_dir` and add `local_timestamp`**

In `src/runtime/mod.rs` `run()`, add `manage_dir: prefs.manage_dir(),` to the `Model { ... }` initializer.

Add a helper (near `map_event`):

```rust
/// Current local time as `YYYYMMDD-HHMMSS` via the Win32 `GetLocalTime` API
/// (no timezone-database dependency).
fn local_timestamp() -> String {
    use windows_sys::Win32::System::SystemInformation::GetLocalTime;
    // SAFETY: GetLocalTime fills a caller-owned SYSTEMTIME; zeroed is valid input.
    let mut st: windows_sys::Win32::Foundation::SYSTEMTIME = unsafe { std::mem::zeroed() };
    unsafe { GetLocalTime(&mut st) };
    format!(
        "{:04}{:02}{:02}-{:02}{:02}{:02}",
        st.wYear, st.wMonth, st.wDay, st.wHour, st.wMinute, st.wSecond
    )
}
```

> If `cargo build` reports a different module path for `SYSTEMTIME`/`GetLocalTime` in the installed `windows-sys`, follow the compiler's suggestion (these live under `Win32::Foundation` / `Win32::System::SystemInformation` in 0.59).

- [ ] **Step 3: Handle the new commands in the event loop**

In `event_loop`, inside `for command in update(model, action)`, add arms (alongside `LaunchInlineShell` etc.), before the `spawnable => dispatch(...)` catch-all:

```rust
                Command::OpenExportDialog { distro } => {
                    let filename = crate::manage::export_filename(&distro, &local_timestamp());
                    let _ = action_tx.send(Action::ExportDialogReady { distro, filename });
                }
                Command::ListExports => {
                    let root = model.manage_dir.clone();
                    let tx = action_tx.clone();
                    tokio::spawn(async move {
                        let entries = tokio::task::spawn_blocking(move || {
                            crate::manage::list_exports(&root).unwrap_or_default()
                        })
                        .await
                        .unwrap_or_default();
                        let _ = tx.send(Action::ExportsListed(entries));
                    });
                }
                Command::DeleteExport(path) => {
                    let root = model.manage_dir.clone();
                    let tx = action_tx.clone();
                    let name = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or_default()
                        .to_string();
                    tokio::spawn(async move {
                        let del = tokio::task::spawn_blocking({
                            let path = path.clone();
                            move || crate::manage::delete_export(&path)
                        })
                        .await;
                        match del {
                            Ok(Ok(())) => {
                                let entries = tokio::task::spawn_blocking(move || {
                                    crate::manage::list_exports(&root).unwrap_or_default()
                                })
                                .await
                                .unwrap_or_default();
                                let _ = tx.send(Action::ExportsListed(entries));
                                let _ = tx.send(Action::OpDone(tf(lang, Key::DoneDeletedArchive, &[&name])));
                            }
                            Ok(Err(error)) => {
                                let _ = tx.send(Action::OpFailed(tf(lang, Key::FailOp, &[&error.to_string()])));
                            }
                            Err(error) => {
                                let _ = tx.send(Action::OpFailed(tf(lang, Key::FailOp, &[&error.to_string()])));
                            }
                        }
                    });
                }
```

- [ ] **Step 4: Keep `dispatch` exhaustive and update long-op wiring**

In `dispatch`, add the three new variants to the no-op list (they are handled inline above):

```rust
        Command::LaunchInlineShell(_)
        | Command::LaunchTabShell(_)
        | Command::Export { .. }
        | Command::Import { .. }
        | Command::Install { .. }
        | Command::CancelOp
        | Command::SavePrefs
        | Command::OpenExportDialog { .. }
        | Command::ListExports
        | Command::DeleteExport(_) => {}
```

In `spawn_long_op`, update the `Export`/`Import` arms to the new fields and create the target directory first:

```rust
            Command::Export { name, path, format } => {
                if let Some(parent) = path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                (
                    tf(lang, Key::DoneExported, &[name]),
                    backend.export(name, path, *format).await,
                )
            }
            Command::Import { name, dir, tar, vhd } => {
                let _ = std::fs::create_dir_all(dir);
                (
                    tf(lang, Key::DoneImported, &[name]),
                    backend.import(name, dir, tar, *vhd).await,
                )
            }
```

(`format`/`vhd` are bound by reference in the `match &command`; deref with `*`.)

- [ ] **Step 5: Build everything**

Run: `cargo build`
Expected: PASS (first fully-green build — backend, message, modal, runtime all consistent).

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock src/runtime/mod.rs
git commit -m "feat(runtime): managed-folder dispatch, local-time names, mkdir"
```

---

## Task 10: reducer — key handlers and new actions

**Files:**
- Modify: `src/app/update/mod.rs`

- [ ] **Step 1: Write the failing tests**

In `src/app/update/mod.rs` `tests` module add:

```rust
    #[test]
    fn e_opens_export_dialog_command() {
        let mut m = model_with(&["Debian"]);
        let cmds = update(&mut m, ch('e'));
        assert_eq!(cmds, vec![Command::OpenExportDialog { distro: "Debian".into() }]);
        assert!(m.modal.is_none(), "form opens only after ExportDialogReady");
    }

    #[test]
    fn export_dialog_ready_opens_form() {
        let mut m = model_with(&["Debian"]);
        update(
            &mut m,
            Action::ExportDialogReady { distro: "Debian".into(), filename: "Debian-20260607-153012.tar".into() },
        );
        match &m.modal {
            Some(Modal::Form(form)) => assert_eq!(form.value(0), "Debian-20260607-153012.tar"),
            other => panic!("expected export form, got {other:?}"),
        }
    }

    #[test]
    fn m_requests_export_listing() {
        let mut m = model_with(&["Debian"]);
        assert_eq!(update(&mut m, ch('m')), vec![Command::ListExports]);
    }

    #[test]
    fn exports_listed_opens_picker() {
        use crate::manage::Archive;
        let mut m = Model::default();
        update(
            &mut m,
            Action::ExportsListed(vec![Archive {
                name: "Ubuntu.tar".into(),
                path: std::path::PathBuf::from(r"C:\wsl\exports\Ubuntu.tar"),
                size: 10,
            }]),
        );
        assert!(matches!(m.modal, Some(Modal::ImportPick(_))));
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test --lib app::update::tests::e_opens_export_dialog_command -v`
Expected: FAIL — `OpenExportDialog` not produced (old `open_export_form` still runs).

- [ ] **Step 3: Update the list-key handlers and the `update` actions**

In `handle_list_key`, replace the `e`/`m` arms:

```rust
        (KeyCode::Char('e'), _) => return open_export(model),
        (KeyCode::Char('m'), _) => return vec![Command::ListExports],
```

Replace `open_export_form`/`open_import_form` with:

```rust
fn open_export(model: &mut Model) -> Vec<Command> {
    let Some(distro) = selected_name(model) else {
        return vec![];
    };
    vec![Command::OpenExportDialog { distro }]
}
```

In the top-level `update()` match, add arms (next to `OnlineList`/`ConfigLoaded`):

```rust
        Action::ExportDialogReady { distro, filename } => {
            model.modal = Some(Modal::Form(FormState::export(distro, filename)));
            vec![]
        }
        Action::ExportsListed(entries) => {
            model.modal = Some(Modal::ImportPick(ImportPickState::new(entries)));
            vec![]
        }
```

Add `ImportPickState` to the `use super::{...}` import list at the top of `update/mod.rs`.

- [ ] **Step 4: Run the tests**

Run: `cargo test --lib app::update::tests -v`
Expected: PASS. (The old `e_opens_export_form`/`import_*` tests that referenced the removed form are updated in Task 11; if any old test fails to compile, comment it for now and replace in Task 11 Step 1.)

- [ ] **Step 5: Commit**

```bash
git add src/app/update/mod.rs
git commit -m "feat(app): e opens export dialog, m lists managed archives"
```

---

## Task 11: reducer — picker keys and submit paths

**Files:**
- Modify: `src/app/update/modal.rs`
- Modify: `src/app/update/mod.rs` (replace obsolete export/import form tests)

- [ ] **Step 1: Write the failing tests**

Add to `src/app/update/mod.rs` `tests` (and delete the now-obsolete `e_opens_export_form`, `import_form_requires_all_fields`, `import_existing_name_asks_to_overwrite`, `import_new_name_skips_overwrite_confirm`, `export_form_submit_dispatches_and_shows_progress` tests, which referenced the removed multi-field import form / non-managed export path):

```rust
    fn archive(name: &str) -> crate::manage::Archive {
        crate::manage::Archive {
            name: name.into(),
            path: std::path::PathBuf::from(format!(r"C:\wsl\exports\{name}")),
            size: 1,
        }
    }

    fn open_picker(names: &[&str]) -> Model {
        let mut m = model_with(&["Debian"]);
        m.manage_dir = std::path::PathBuf::from(r"C:\wsl");
        update(&mut m, Action::ExportsListed(names.iter().map(|n| archive(n)).collect()));
        m
    }

    #[test]
    fn export_submit_builds_managed_path_and_format() {
        let mut m = model_with(&["Debian"]);
        m.manage_dir = std::path::PathBuf::from(r"C:\wsl");
        update(&mut m, Action::ExportDialogReady { distro: "Debian".into(), filename: "Debian.tar.gz".into() });
        let cmds = update(&mut m, key(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(
            cmds,
            vec![Command::Export {
                name: "Debian".into(),
                path: std::path::PathBuf::from(r"C:\wsl\exports\Debian.tar.gz"),
                format: crate::manage::ExportFormat::TarGz,
            }]
        );
        assert!(matches!(m.modal, Some(Modal::Progress(_))));
    }

    #[test]
    fn picker_enter_opens_name_form_prefilled() {
        let mut m = open_picker(&["Ubuntu-20260607.tar.gz"]);
        update(&mut m, key(KeyCode::Enter, KeyModifiers::NONE));
        match &m.modal {
            Some(Modal::Form(form)) => assert_eq!(form.value(0), "Ubuntu-20260607"),
            other => panic!("expected import name form, got {other:?}"),
        }
    }

    #[test]
    fn picker_import_dispatches_with_managed_dir_and_vhd_flag() {
        let mut m = open_picker(&["box.vhdx"]);
        update(&mut m, key(KeyCode::Enter, KeyModifiers::NONE)); // -> name form ("box")
        let cmds = update(&mut m, key(KeyCode::Enter, KeyModifiers::NONE)); // submit
        assert_eq!(
            cmds,
            vec![Command::Import {
                name: "box".into(),
                dir: std::path::PathBuf::from(r"C:\wsl\installed\box"),
                tar: std::path::PathBuf::from(r"C:\wsl\exports\box.vhdx"),
                vhd: true,
            }]
        );
        assert!(matches!(m.modal, Some(Modal::Progress(_))));
    }

    #[test]
    fn picker_d_asks_to_delete() {
        let mut m = open_picker(&["old.tar"]);
        let cmds = update(&mut m, ch('d'));
        assert!(cmds.is_empty(), "delete is confirmed first");
        assert!(matches!(m.modal, Some(Modal::Confirm(_))));
        let cmds = update(&mut m, ch('y'));
        assert_eq!(cmds, vec![Command::DeleteExport(std::path::PathBuf::from(r"C:\wsl\exports\old.tar"))]);
    }

    #[test]
    fn picker_c_opens_custom_form() {
        let mut m = open_picker(&["a.tar"]);
        update(&mut m, ch('c'));
        match &m.modal {
            Some(Modal::Form(form)) => assert_eq!(form.fields.len(), 2),
            other => panic!("expected custom import form, got {other:?}"),
        }
    }

    #[test]
    fn picker_esc_closes() {
        let mut m = open_picker(&["a.tar"]);
        let cmds = update(&mut m, key(KeyCode::Esc, KeyModifiers::NONE));
        assert!(cmds.is_empty());
        assert!(m.modal.is_none());
    }

    #[test]
    fn import_existing_name_asks_overwrite() {
        let mut m = open_picker(&["Debian.tar"]); // model_with(["Debian"]) already exists
        update(&mut m, key(KeyCode::Enter, KeyModifiers::NONE)); // name form prefilled "Debian"
        let cmds = update(&mut m, key(KeyCode::Enter, KeyModifiers::NONE));
        assert!(cmds.is_empty(), "must confirm overwrite");
        assert!(matches!(m.modal, Some(Modal::Confirm(_))));
        let cmds = update(&mut m, ch('y'));
        assert!(matches!(cmds.as_slice(), [Command::Import { .. }]));
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test --lib app::update::tests::export_submit_builds_managed_path_and_format -v`
Expected: FAIL — submit still uses the old `FormKind::Export` path field / `handle_import_pick_key` missing.

- [ ] **Step 3: Add the import-pick handler and rewrite submit**

In `src/app/update/modal.rs`, add the picker dispatch arm in `handle_modal_key`:

```rust
        Modal::ImportPick(pick) => handle_import_pick_key(model, pick, key),
```

Add the handler:

```rust
fn handle_import_pick_key(
    model: &mut Model,
    mut pick: ImportPickState,
    key: KeyEvent,
) -> Vec<Command> {
    match key.code {
        KeyCode::Esc => vec![], // cancelled (modal already taken)
        KeyCode::Down | KeyCode::Char('j') => {
            pick.select_next();
            model.modal = Some(Modal::ImportPick(pick));
            vec![]
        }
        KeyCode::Up | KeyCode::Char('k') => {
            pick.select_prev();
            model.modal = Some(Modal::ImportPick(pick));
            vec![]
        }
        KeyCode::Enter => {
            if let Some(entry) = pick.selected_entry() {
                let tar = entry.path.clone();
                let default_name = crate::manage::derive_distro_name(&entry.name);
                model.modal = Some(Modal::Form(FormState::import_name(tar, default_name)));
            } else {
                model.modal = Some(Modal::ImportPick(pick));
            }
            vec![]
        }
        KeyCode::Char('c') => {
            model.modal = Some(Modal::Form(FormState::import_custom()));
            vec![]
        }
        KeyCode::Char('d') => {
            if let Some(entry) = pick.selected_entry() {
                let prompt = tf(model.lang, Key::PromptDeleteArchive, &[&entry.name]);
                let path = entry.path.clone();
                model.modal = Some(Modal::Confirm(Confirm {
                    prompt,
                    require_typed: None,
                    on_confirm: vec![Command::DeleteExport(path)],
                    progress_title: None,
                    status: None,
                }));
            } else {
                model.modal = Some(Modal::ImportPick(pick));
            }
            vec![]
        }
        _ => {
            model.modal = Some(Modal::ImportPick(pick));
            vec![]
        }
    }
}
```

Replace `submit_form` and add `submit_import`:

```rust
fn submit_form(model: &mut Model, form: FormState) -> Vec<Command> {
    match form.kind.clone() {
        FormKind::Export { distro } => {
            let filename = form.value(0).trim().to_string();
            if filename.is_empty() {
                model.modal = Some(Modal::Form(form));
                return vec![];
            }
            let path = crate::manage::exports_dir(&model.manage_dir).join(&filename);
            let format = crate::manage::ExportFormat::from_filename(&filename);
            let title = tf(model.lang, Key::ProgExporting, &[&distro]);
            model.modal = Some(Modal::Progress(ProgressState::new(title)));
            vec![Command::Export { name: distro, path, format }]
        }
        FormKind::ImportName { tar } => {
            let name = form.value(0).trim().to_string();
            if name.is_empty() {
                model.modal = Some(Modal::Form(form));
                return vec![];
            }
            submit_import(model, name, tar)
        }
        FormKind::ImportCustom => {
            let tar = form.value(0).trim().to_string();
            let name = form.value(1).trim().to_string();
            if tar.is_empty() || name.is_empty() {
                model.modal = Some(Modal::Form(form));
                return vec![];
            }
            submit_import(model, name, PathBuf::from(tar))
        }
    }
}

fn submit_import(model: &mut Model, name: String, tar: PathBuf) -> Vec<Command> {
    let dir = crate::manage::installed_dir(&model.manage_dir, &name);
    let vhd = crate::manage::is_vhd_archive(&tar.to_string_lossy());
    let title = tf(model.lang, Key::ProgImporting, &[&name]);
    let import = Command::Import { name: name.clone(), dir, tar, vhd };
    if model
        .distros
        .iter()
        .any(|distro| distro.name.eq_ignore_ascii_case(&name))
    {
        model.modal = Some(Modal::Confirm(Confirm {
            prompt: tf(model.lang, Key::PromptImportOverwrite, &[&name]),
            require_typed: None,
            on_confirm: vec![import],
            progress_title: Some(title),
            status: None,
        }));
        return vec![];
    }
    model.modal = Some(Modal::Progress(ProgressState::new(title)));
    vec![import]
}
```

`handle_import_pick_key` uses `ImportPickState` — it is already in scope via `use super::*;` in `modal.rs` provided `update/mod.rs` imports it (Task 10 Step 3). Confirm `ImportPickState` is in the `use super::{...}` list.

- [ ] **Step 4: Run the tests**

Run: `cargo test --lib app::update -v`
Expected: PASS (new tests green; obsolete ones removed in Step 1).

- [ ] **Step 5: Commit**

```bash
git add src/app/update/modal.rs src/app/update/mod.rs
git commit -m "feat(app): import picker keys; managed export/import submit"
```

---

## Task 12: UI — render the picker and the export hint

**Files:**
- Modify: `src/ui/mod.rs`

- [ ] **Step 1: Write the failing UI test**

In `src/ui/mod.rs` `tests`, add a render test for the picker (model the assertion on the existing `renders_install_pick_modal` test style):

```rust
    #[test]
    fn renders_import_pick_modal() {
        use crate::app::ImportPickState;
        let mut model = Model::default();
        model.modal = Some(Modal::ImportPick(ImportPickState::new(vec![crate::manage::Archive {
            name: "Ubuntu-20260607.tar.gz".into(),
            path: std::path::PathBuf::from(r"C:\wsl\exports\Ubuntu-20260607.tar.gz"),
            size: 1024,
        }])));
        let backend = ratatui::backend::TestBackend::new(100, 30);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|f| view(f, &model)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let text: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(text.contains("Ubuntu-20260607.tar.gz"), "picker should list the archive");
    }
```

(Match the imports/helpers the existing UI tests use; if they construct `Terminal`/`TestBackend` via a local helper, reuse it.)

- [ ] **Step 2: Run to verify failure**

Run: `cargo test --lib ui::tests::renders_import_pick_modal -v`
Expected: FAIL — `Modal::ImportPick` not handled in `render_modal` (non-exhaustive match or missing render).

- [ ] **Step 3: Add the renderer and dispatch**

In `src/ui/mod.rs`, add `ImportPickState` to the `use crate::app::{...}` import list, and ensure `human_size` is in scope (it lives in `ui::util`; the detail pane already uses it).

Add the dispatch arm in `render_modal`:

```rust
        Modal::ImportPick(pick) => render_import_pick(f, pick, lang, area),
```

Add the renderer (after `render_install_pick`):

```rust
fn render_import_pick(f: &mut Frame, pick: &ImportPickState, lang: Lang, area: Rect) {
    let popup = centered_rect(74, 22, area);
    f.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(t(lang, Key::PickImportTitle));
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let rows = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(inner);

    if pick.entries.is_empty() {
        f.render_widget(Paragraph::new(t(lang, Key::PickImportEmpty)), rows[0]);
    } else {
        let items: Vec<ListItem> = pick
            .entries
            .iter()
            .map(|a| ListItem::new(format!("{:<44} {}", a.name, human_size(a.size))))
            .collect();
        let list = List::new(items)
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
            .highlight_symbol("▶ ");
        let mut state = ListState::default();
        state.select(Some(pick.selected.min(pick.entries.len() - 1)));
        f.render_stateful_widget(list, rows[0], &mut state);
    }

    f.render_widget(Paragraph::new(t(lang, Key::PickImportHints)), rows[1]);
}
```

Update `render_form`'s title match and add the export hint:

```rust
    let title = t(
        lang,
        match &form.kind {
            FormKind::Export { .. } => Key::FormExportTitle,
            FormKind::ImportName { .. } | FormKind::ImportCustom => Key::FormImportTitle,
        },
    );
```

After the loop that builds `text` and appends `FormFooter`, append the export hint for the export form:

```rust
    if matches!(form.kind, FormKind::Export { .. }) {
        text.push('\n');
        text.push_str(t(lang, Key::ExportFormatHint));
    }
```

(If `human_size` is not already imported in `ui/mod.rs`, add `use util::human_size;` — match how the detail pane imports it.)

If any existing UI test constructed the removed 3-field import form (`FormState::import()`), update it to `FormState::import_name(...)`/`import_custom()` or remove it (the export-form test using `FormState::export` is unaffected).

- [ ] **Step 4: Run the UI tests**

Run: `cargo test --lib ui:: -v`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/ui/mod.rs
git commit -m "feat(ui): render the import picker and export format hint"
```

---

## Task 13: Help text + README

**Files:**
- Modify: `src/i18n/mod.rs` (help line)
- Modify: `README.md`

- [ ] **Step 1: Update the help overlay strings**

In `src/i18n/mod.rs`, find the full help text keys (`HelpFull` / the multi-line help string with `e .tar にエクスポート` / `m .tar からインポート`). Update those two lines to reflect the new behaviour, e.g.:

- en: `e   export to the managed folder` / `m   import (pick from the managed folder)`
- ja: `e   管理フォルダにエクスポート` / `m   インポート（管理フォルダから選択）`

Keep both languages in sync; do not add new keys (edit the existing help strings).

- [ ] **Step 2: Update README export/import section**

In `README.md`, update the export/import description to: export saves into `%USERPROFILE%\wsl-manager\exports\` (extension picks the format: `.tar`/`.tar.gz`/`.tar.xz`/`.vhdx`); import opens a picker over that folder (`c` for a custom path, `d` to delete); imported distros are stored under `installed\<name>\`; the root is configurable via `manage_dir` in `config.toml`.

- [ ] **Step 3: Verify build + format**

Run: `cargo fmt --all -- --check` and `cargo test --lib i18n -v`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add src/i18n/mod.rs README.md
git commit -m "docs: help/README for the managed export/import folder"
```

---

## Task 14: Full verification

**Files:** none (gates only)

- [ ] **Step 1: Format check**

Run: `cargo fmt --all -- --check`
Expected: clean (exit 0).

- [ ] **Step 2: Clippy (warnings = errors)**

Run: `cargo clippy --all-targets -- -D warnings`
Expected: no warnings.

- [ ] **Step 3: All tests**

Run: `cargo test --all`
Expected: all pass; 2 ignored (the existing real-WSL integration tests).

- [ ] **Step 4: Manual smoke (real WSL host)**

Run `cargo run`, then:
- `e` on a distro → confirm dialog pre-filled `<name>-<localtime>.tar`; edit extension to `.tar.gz`; Enter → archive appears in `%USERPROFILE%\wsl-manager\exports\`.
- `m` → picker lists the archive; `Enter` → type a name → imports into `installed\<name>\`; the new distro appears in the list.
- `m` → select an archive → `d` → confirm → it disappears from the picker.
- `m` → `c` → type an external `.tar.gz` path + name → imports.

- [ ] **Step 5: Final commit (if any fixups)**

```bash
git add -A
git commit -m "chore: managed export/import folder — final fixups"
```

---

## Notes for the implementer

- **Build ordering:** Tasks 4–9 change types across files; the first fully-green `cargo build` is at the **end of Task 9**. Within Tasks 4–8 use the scoped `cargo build 2>&1 | rg "<file>"` checks noted in each task rather than expecting a clean whole-crate build.
- **Pure vs IO:** never call the filesystem or `GetLocalTime` from `src/app/**` (the reducer). Those live in `src/manage` (fs) and `src/runtime` (clock + dispatch).
- **Keep `Key::ALL` in sync** whenever you add an i18n key (the `every_key_has_both_languages` test enforces it).
- **Don't regress** the inline-shell `EventStream` drop/recreate or the transient-status behaviour on the other branch — this feature branch is independent of `fix/inline-shell-input-and-transient-status`.
