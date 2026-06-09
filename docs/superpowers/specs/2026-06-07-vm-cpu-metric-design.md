# VM CPU metric in the detail pane — design spec

Date: 2026-06-07
Status: Approved (pending written-spec review)
Component: `wslm` (wsl-manager-tui)

## 1. Motivation

The detail pane already surfaces the WSL2 VM's memory (`vmmemWSL` working set) as a
single machine-wide figure plus a sparkline trend. Users also want to see how busy the
VM is on the CPU. This spec adds a **VM CPU usage** line and its own trend sparkline,
mirroring the existing VM-memory treatment.

WSL2 runs **all distributions inside one shared VM** (one `vmmemWSL` host process), so
CPU — like memory — is reported as a single shared figure, not per-distro. Per-distro
attribution is impossible: every distro shares the same VM and kernel, so neither the
Windows side nor in-distro `/proc/stat` can separate one distro's CPU from another's.

## 2. Goals / non-goals

**Goals**
- Show **VM-wide CPU usage** of the WSL2 VM (`vmmemWSL`) in the detail pane.
- Display as **`VM CPU: NN.N %`** plus a **dedicated trend sparkline**, alongside the
  existing VM-memory line and sparkline.
- **100% = the whole host's logical CPUs fully used** (Task Manager parity for the
  `vmmemWSL` process). E.g. on an 8-logical-CPU host, the VM using 4 cores' worth reads
  50%; all cores saturated reads 100%.
- Reuse the existing metrics MVU path (poll tick → sample → history → render). **No new
  `Command`/`Action` variants.**

**Non-goals (v1, YAGNI)**
- Per-distro CPU (impossible — shared VM/kernel; see §1).
- Host-wide CPU (out of scope for a WSL manager).
- A configurable CPU normalization basis. Host-logical-CPU is the only basis.
- Toggling the two sparklines into one shared row — both are shown (height permitting).

## 3. What "CPU%" measures and how it is computed

`vmmemWSL` is a **protected minimal process**: it cannot be opened even with
`PROCESS_QUERY_LIMITED_INFORMATION`, so every handle-based reader (sysinfo's
`process.cpu_usage()`, `GetProcessTimes`) reports nothing for it — the same constraint
that forced the memory reading onto `NtQuerySystemInformation`. **CPU must therefore
come from the same `NtQuerySystemInformation(SystemProcessInformation)` snapshot**, which
carries each process's cumulative `KernelTime` and `UserTime` (100 ns units) without a
handle.

CPU% over a sampling interval is a pure function of two consecutive snapshots:

```
Δcpu_100ns = (KernelTime + UserTime)_now − (KernelTime + UserTime)_prev
Δt_100ns   = (taken_at_now − taken_at_prev) in 100 ns units
cpu_pct    = clamp( Δcpu_100ns / (Δt_100ns × logical_cpus) × 100,  0.0 ..= 100.0 )
```

- `logical_cpus` = the host's logical processor count (`std::thread::available_parallelism()`).
  This gives the Task-Manager basis (% of the whole host).
- Using the **actual elapsed time** (`taken_at` deltas) rather than the nominal poll
  interval keeps the figure correct when a tick is delayed under load — exactly when CPU
  is high and the nominal interval would over-report.

## 4. Architecture — changes per layer

The feature rides the existing memory path end-to-end. The only new MVU surface is extra
fields on `MetricsSample`; the reducer is unchanged.

```
poll tick ─► SampleMetrics ─► spawn_blocking(metrics::sample)
          ─► MetricsSampled(sample) ─► update(): model.metrics.push(&sample)   (still pure)
          ─► ui::render_detail reads model.metrics
```

### 4.1 `src/metrics/mod.rs` (the IO boundary)

- **Snapshot read**: extend `enumerate_vm_candidates` to return
  `(VmKind, working_set_bytes, cpu_100ns)` where `cpu_100ns = KernelTime + UserTime`.
  **Important:** `windows-sys` 0.59 ships the *documented* (winternl.h) form of
  `SYSTEM_PROCESS_INFORMATION`, in which `WorkingSetSize` is a named field but the CPU
  times are **not** — they live inside the `Reserved1: [u8; 48]` blob. That blob is the
  stable (Vista+) sequence `WorkingSetPrivateSize(8) · HardFaultCount(4) ·
  NumberOfThreadsHighWatermark(4) · CycleTime(8) · CreateTime(8) · UserTime(8) ·
  KernelTime(8)`, so `UserTime = Reserved1[32..40]` and `KernelTime = Reserved1[40..48]`,
  each read as a little-endian `i64` via `i64::from_le_bytes`. They are summed (order
  between the two is irrelevant to the sum) and clamped to `u64` at `≥ 0`. A small helper
  `vm_cpu_100ns(&info) -> u64` keeps the offset arithmetic in one documented place.
- **Selection**: add `select_vm_cpu_ticks(&candidates) -> Option<u64>`, symmetric with the
  existing `select_vm_working_set` (prefer `vmmemWSL` over legacy `vmmem`).
- **`MetricsSample`** gains:
  - `vm_cpu_100ns: Option<u64>` — raw cumulative CPU time of the VM (`None` ⇒ VM not
    running / query carried no VM process).
  - `taken_at: Option<Instant>` — when the snapshot was read.
  - `logical_cpus: u32` — host logical CPU count at sample time.
  - All three are `Default`-able (`None`/`None`/`0`), so `MetricsSample`'s existing
    `#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]` is **kept** (`Option<Instant>`
    is `Copy + Eq + Default`).
- **`sample()`** stamps `taken_at = Some(Instant::now())` and
  `logical_cpus = available_parallelism()` alongside the existing reads. `Instant::now()`
  lives only here, at the IO boundary (this fn already does side-effectful reads).
- **`MetricsHistory`** gains:
  - `latest_vm_cpu_pct: Option<f32>` — most recent computed percentage (`None` until a
    second sample exists, or when the VM is stopped).
  - a `vm_cpu` ring buffer + `cpu_sparkline() -> Vec<u64>` (stored as `pct × 10` rounded,
    0..=1000, for finer sparkline resolution; a stopped VM pushes 0, matching memory).
  - internal `prev_cpu: Option<(u64 /*100ns*/, Instant)>` carried between pushes.
  - **CPU% is computed inside `MetricsHistory::push`** — pure arithmetic over the two
    `Instant`s and tick counts it is handed (no clock reads, no IO), so it stays the
    metrics layer's unit-test surface.

### 4.2 `src/app/` (the pure reducer)

No new fields, variants, or wiring. `Action::MetricsSampled(sample)` still calls
`model.metrics.push(&sample)`, which now also folds in CPU. The reducer remains pure.

### 4.3 `src/ui/mod.rs` (render only)

- Add a **`VM CPU: NN.N %`** line to the info block (formatted from
  `model.metrics.latest_vm_cpu_pct`; `—` when `None`).
- Restructure the detail pane's bottom from one sparkline row to **two `Length(1)` rows**:
  memory trend, then CPU trend, each with its short label (a shared
  `render_sparkline_row` helper, DRY). The info block keeps `Constraint::Min(1)`.
- **Grow the detail pane** in `view()` from `Constraint::Length(9)` to
  `Constraint::Length(11)` (interior 9 rows): the worst case is 7 info lines
  (State, Version+Default, Disk, Path, VM Mem, VM CPU, In-distro) + 2 trend rows = 9.
  The table above is `Constraint::Min(5)`, so it simply yields the 2 extra rows.
- **Height guard**: if the interior is too short for two trend rows (tiny terminals
  where ratatui cannot honor `Length(11)`), render only the memory sparkline (never
  panic / overlap). Width math stays CJK-aware via `unicode-width`.

### 4.4 `src/i18n/mod.rs`

- Add `Key::DetailVmCpu` → (`"VM CPU"`, `"VM CPU"`).
- Add `Key::DetailVmCpuTrend` → (`"CPU"`, `"CPU推移"`).
- Relabel `Key::DetailVmMemTrend` from (`"Trend"`,`"推移"`) to (`"Mem"`, `"メモリ推移"`)
  so the two trends are distinguishable.
- Keep `Key::ALL` in sync; the `every_key_has_both_languages` test must stay green.

## 5. Error handling / edge cases

- **Snapshot query fails** (`enumerate_vm_candidates` → `None`): `sample()` returns `None`;
  the runtime keeps the previous reading (existing behavior). No CPU flip on a transient
  hiccup.
- **First sample**: no previous snapshot ⇒ `latest_vm_cpu_pct = None`, shown as `—`. The
  next tick yields the first percentage (startup priming already dispatches one
  `SampleMetrics`, so this resolves within one poll interval).
- **VM stopped** (`vm_cpu_100ns = None`): `latest_vm_cpu_pct = None` (`—`); the CPU
  sparkline records 0, matching how memory shows the dip.
- **Counter quirks**: `Δcpu` clamped at ≥ 0; `cpu_pct` clamped to `0.0..=100.0` to absorb
  scheduling jitter and the (host > VM-assigned) processor mismatch when `.wslconfig`
  limits `processors=`.

## 6. Testing (TDD)

- **`select_vm_cpu_ticks`**: prefers `vmmemWSL` over legacy `vmmem`; `None` when neither —
  symmetric with the existing `select_vm_working_set` tests.
- **`MetricsHistory::push` CPU math**: feed two samples with `taken_at: Some(t0)` and
  `Some(t0 + Duration::from_secs(1))` and known `vm_cpu_100ns`/`logical_cpus`; assert the
  expected `latest_vm_cpu_pct`. Cover: first-sample `None`, stopped-VM `None` + 0 in
  sparkline, and clamping (Δ that would exceed 100%).
- **UI**: extend the detail-pane render test (à la `renders_detail_pane_with_vm_memory`)
  to assert the `VM CPU` line and a second sparkline render on a `TestBackend`; add a
  short-height case asserting no panic and memory-only fallback.
- **Real-machine (`#[ignore]`)**: assert `vmmemWSL` cumulative CPU time is monotonically
  non-decreasing across two real snapshots.

## 7. Out-of-scope follow-ups

- Per-distro CPU estimation (would be an approximation; explicitly rejected).
- Persisting CPU history across runs.
- A combined memory+CPU single-row toggle.
