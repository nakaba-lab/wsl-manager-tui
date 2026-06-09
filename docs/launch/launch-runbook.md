# Launch runbook (one concentrated push)

**Precondition:** product is feature-complete + polished, a real
`v1.0.0` release is published, Scoop/winget/crates.io install paths work. Do NOT launch
before these — you get one first impression per community and cannot re-"Show HN" the same v1.

**Timing:** Tue–Thu, ~9:00–12:00 ET (14:00–17:00 UTC). Be free for the next 1–2 hours.

**Calibration:** ~93% of Show HN posts never hit 50 points (median ~2), and HN skews
Unix/Mac against a Windows-only tool. Expect a modest, real spike from the subreddits;
treat a front-page HN hit as upside, not the plan.

## Order of operations (single day)

1. **Hacker News — Show HN** (post the GitHub URL as the link).
   - Title (plain, no caps/hype/numbers):
     `Show HN: wslm – a terminal UI to manage WSL distributions`
   - Immediately add this as the first comment:

     > Hi HN — I built wslm, a single-binary terminal UI (Rust + ratatui) for managing WSL
     > distributions on Windows. From one screen it lists distros with state/version/disk,
     > starts/stops them, opens shells (inline or in a new Windows Terminal tab), shows live
     > WSL VM memory/CPU gauges with sparklines, does managed export/import (tar/vhdx),
     > installs from the online catalog, and edits .wslconfig / wsl.conf.
     >
     > Why a TUI when Windows has GUIs? It runs over SSH, in a tmux pane, or on a GUI-less
     > Server Core box; it's one self-contained .exe with no telemetry; and it's MIT/Apache-2.0.
     > State detection is locale-independent — it never parses localized wsl.exe text, so it's
     > correct under any Windows display language.
     >
     > It's my first real OSS release. Feedback very welcome, especially on UX and anything
     > that feels un-idiomatic for terminal tools.

2. **Reply substantively to every comment for the first 1–2 hours** (comments outrank
   upvotes on HN). Be humble, concrete, non-defensive.

3. **Reddit** — stagger same-day → next-day, each post **tailored** (never copy-paste the
   same text across subs; never cross-link or ask for upvotes — both get detected):
   - **r/bashonubuntuonwindows** (WSL-user angle):
     > Made a terminal UI to manage WSL distros — list/start/stop, inline shells, live VM
     > memory/CPU, export/import, and a .wslconfig/wsl.conf editor, all in one screen.
     > Single MIT/Apache .exe, no telemetry, works over SSH. Feedback welcome: <url>
   - **r/commandline** (TUI angle):
     > wslm — a ratatui TUI to manage WSL from the terminal: distro list with live gauges
     > + sparklines, inline/tab shells, export/import, config editor. Single self-contained
     > exe. Would love TUI-UX feedback: <url>
   - **r/rust** (built-with angle — post as a "project"/"media" per sub rules):
     > Built a WSL manager TUI in Rust + ratatui (async MVU, locale-independent state
     > detection, CJK-aware width math). Single stripped LTO exe. Source + notes: <url>
   - Optional: **r/coolgithubprojects**.

## Do / Don't

- DO keep the value prop above the fold (the ASCII UI frame leads the README); DO pin it to
  "terminal-native + single-exe + permissive".
- DON'T ask for/coordinate upvotes, DON'T spam the same link across many subs at once,
  DON'T argue with critics.

## Evergreen discovery submissions (one-time; do around launch, no deadline)

- [ ] awesome-wsl (PR): add under the management section.
- [ ] awesome-ratatui (PR): add under apps.
- [ ] awesome-tuis (PR): add under the relevant category.
- [ ] Terminal Trove → "Post a Tool".
- [ ] console.dev → submit a tool (Thursday devtools newsletter).

  Suggested awesome-list one-liner:
  > **[wslm](https://github.com/nakaba-lab/wsl-manager-tui)** — single-binary terminal UI
  > (ratatui) to manage WSL distributions: lifecycle, shells, live VM memory/CPU,
  > export/import, and .wslconfig/wsl.conf editing. Windows, MIT/Apache-2.0.

## Install-path setup (do BEFORE launch so "install" is a one-liner on arrival)

- [ ] **Scoop bucket:** create a `scoop-wslm` repo, copy `packaging/scoop/wslm.json` in
      with the real `version`/`hash`, so `scoop bucket add wslm <url>; scoop install wslm` works.
- [ ] **winget:** `wingetcreate new https://github.com/nakaba-lab/wsl-manager-tui/releases/download/v1.0.0/wslm-x86_64-pc-windows-msvc.exe`
      → review the generated manifests (portable installer type) → submit the PR to microsoft/winget-pkgs.
- [ ] **crates.io:** `cargo publish` (enables `cargo install wsl-manager-tui` → `wslm`).
