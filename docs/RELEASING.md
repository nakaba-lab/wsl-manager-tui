# Releasing wslm

## Cut a release

1. Bump `version` in `Cargo.toml`; run `cargo build` to update `Cargo.lock`.
2. Commit: `chore(release): vX.Y.Z`.
3. Tag and push — this triggers `.github/workflows/release.yml`, which builds and
   publishes `wslm-x86_64-pc-windows-msvc.exe`, `SHA256SUMS`, and the license files:
   ```sh
   git tag vX.Y.Z
   git push origin vX.Y.Z
   ```
4. Edit the published GitHub Release notes.

## ALWAYS, in every release's notes (the one recurring monetization habit)

Append this block to every release's notes — re-surfacing the ask on each release is the
single empirically-supported lever that turns a flatlined tip jar into a slow trickle:

> 💚 If wslm is useful to you, please consider sponsoring continued development:
> [GitHub Sponsors](https://github.com/sponsors/nakata5577) · [Ko-fi](https://ko-fi.com/nakata5577)

## After a release — update the package managers

- **Scoop:** bump `version`/`hash`/`url` in `bucket/wslm.json` in the
  [nakaba-lab/scoop-bucket](https://github.com/nakaba-lab/scoop-bucket) repo (or rely on its
  `autoupdate` block + `checkver`).
- **winget:** `wingetcreate update nakaba-lab.wslm --version X.Y.Z --urls <new-exe-url>`
  then submit the PR (see `docs/launch/launch-runbook.md`).
- **crates.io:** `cargo publish` (only if the crate metadata changed meaningfully).
