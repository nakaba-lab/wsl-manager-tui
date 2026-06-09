# VM CPU metric in the detail pane — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show the WSL2 VM's CPU usage (a single shared figure) in the detail pane as a `VM CPU: NN.N %` line plus a dedicated trend sparkline, mirroring the existing VM-memory treatment.

**Architecture:** Ride the existing metrics MVU path (poll tick → `metrics::sample` → `MetricsSampled` → `MetricsHistory::push` → `ui::render_detail`). CPU time comes from the *same* `NtQuerySystemInformation(SystemProcessInformation)` snapshot already used for memory (`vmmemWSL` is a protected process, so handle-based readers report nothing). `MetricsSample` gains raw cumulative CPU ticks + a timestamp; the percentage is computed by pure arithmetic in `MetricsHistory::push` from two consecutive samples, normalized to the host's logical CPU count (Task Manager parity). No new `Command`/`Action` variants; the reducer is untouched.

**Tech Stack:** Rust 1.96.0, ratatui, `windows-sys` 0.59 (`NtQuerySystemInformation`), `std::time::Instant`, `std::thread::available_parallelism`.

**Spec:** `docs/superpowers/specs/2026-06-07-vm-cpu-metric-design.md`

---

## Reference: current shapes (do not guess — these are the exact starting points)

`src/metrics/mod.rs` — `MetricsSample` (lines ~14-21):

```rust
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MetricsSample {
    pub vmmem_bytes: Option<u64>,
    pub total_mem_bytes: u64,
}
```

`enumerate_vm_candidates` currently returns `Option<Vec<(VmKind, u64)>>`; `select_vm_working_set` takes `&[(VmKind, u64)]`. `MetricsHistory` (lines ~179-187) derives only `#[derive(Debug, Clone)]` (no Eq — so `f32` fields are fine there, never on `MetricsSample`).

`src/ui/mod.rs` `view()` splits the screen vertically: table `Min(5)`, detail `Length(9)`, status `Length(1)`. `render_detail` (lines ~43-114) builds a multi-line info `Paragraph` then one labeled sparkline row.

---

## Task 1: Extend `MetricsSample` with CPU fields (no behavior change)

Adds the three new fields and keeps the whole build green by defaulting them at every existing construction site. No new behavior yet.

**Files:**
- Modify: `src/metrics/mod.rs` (struct + `sample()` + 2 unit-test literals)
- Modify: `src/app/update/mod.rs` (1 test literal)
- Modify: `src/ui/mod.rs` (2 test literals)

- [ ] **Step 1: Add the import and extend the struct**

In `src/metrics/mod.rs`, add to the top `use` block (just after `use std::path::Path;`):

```rust
use std::time::Instant;
```

Replace the `MetricsSample` definition with:

```rust
/// A single resource sample.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MetricsSample {
    /// RSS of the `vmmemWSL` process in bytes, or `None` when the WSL VM is not
    /// running.
    pub vmmem_bytes: Option<u64>,
    /// Total physical memory of the machine, in bytes.
    pub total_mem_bytes: u64,
    /// Cumulative CPU time (kernel + user) of the WSL VM in 100 ns units, or
    /// `None` when the VM is not running. A *percentage* is derived later from
    /// the delta between two consecutive samples (see `MetricsHistory::push`).
    pub vm_cpu_100ns: Option<u64>,
    /// When this sample was taken, used as the time base for the CPU delta.
    /// `None` only in synthetic test samples.
    pub taken_at: Option<Instant>,
    /// Host logical CPU count at sample time; the denominator that makes CPU%
    /// "% of the whole host" (0 ⇒ treated as 1).
    pub logical_cpus: u32,
}
```

(`Option<Instant>`/`Option<u64>`/`u32` are all `Copy + Eq + Default`, so every derive — including `Eq`, needed because `Action::MetricsSampled(MetricsSample)` derives `Eq` — still holds.)

- [ ] **Step 2: Keep `sample()` compiling by defaulting the new fields**

In `src/metrics/mod.rs`, change the `sample()` return literal to spread defaults (real values come in Task 2):

```rust
    Some(MetricsSample {
        vmmem_bytes: select_vm_working_set(&candidates),
        total_mem_bytes: system.total_memory(),
        ..Default::default()
    })
```

- [ ] **Step 3: Default the new fields in the existing test literals**

In `src/metrics/mod.rs`, both `MetricsSample { ... }` literals inside `history_is_a_ring_buffer` and `history_records_stopped_vm_as_zero` get `..Default::default()`. For example:

```rust
            history.push(&MetricsSample {
                vmmem_bytes: Some(value),
                total_mem_bytes: 100,
                ..Default::default()
            });
```

and:

```rust
        history.push(&MetricsSample {
            vmmem_bytes: None,
            total_mem_bytes: 100,
            ..Default::default()
        });
```

In `src/app/update/mod.rs`, the `MetricsSampled` test (around line 497):

```rust
            Action::MetricsSampled(MetricsSample {
                vmmem_bytes: Some(123),
                total_mem_bytes: 8 * 1024 * 1024 * 1024,
                ..Default::default()
            }),
```

In `src/ui/mod.rs`, both test literals in `renders_detail_pane_with_vm_memory` and `vm_memory_denominator_uses_wsl_vm_ram_when_known`:

```rust
        model.metrics.push(&MetricsSample {
            vmmem_bytes: Some(2 * 1024 * 1024 * 1024),
            total_mem_bytes: 8 * 1024 * 1024 * 1024,
            ..Default::default()
        });
```

(the second test additionally keeps its `model.vm_mem_total = Some(...)` line unchanged.)

- [ ] **Step 4: Verify the build and tests are still green**

Run: `cargo test --all`
Expected: PASS (same test count as before; this step adds no new behavior).

- [ ] **Step 5: Commit**

```bash
git add src/metrics/mod.rs src/app/update/mod.rs src/ui/mod.rs
git commit -m "refactor(metrics): add CPU fields to MetricsSample (no behavior change)"
```

---

## Task 2: Read CPU time from the snapshot + `select_vm_cpu_ticks`

Replace the candidate tuple with a named `VmCandidate`, read kernel+user CPU time out of the `Reserved1` blob, and have `sample()` populate the real CPU fields.

**Files:**
- Modify: `src/metrics/mod.rs` (candidate type, enumerate, selectors, `sample()`, tests)

- [ ] **Step 1: Write the failing test for `select_vm_cpu_ticks`**

In `src/metrics/mod.rs` `#[cfg(test)] mod tests`, replace the three existing `select_*` tests (`select_prefers_wsl_over_legacy_vmmem`, `select_falls_back_to_legacy_vmmem`, `select_returns_none_when_no_vm_present`) with versions built on `VmCandidate`, and add CPU selection tests:

```rust
    fn cand(kind: VmKind, working_set: u64, cpu_100ns: u64) -> VmCandidate {
        VmCandidate {
            kind,
            working_set,
            cpu_100ns,
        }
    }

    #[test]
    fn select_prefers_wsl_over_legacy_vmmem() {
        let candidates = [
            cand(VmKind::Legacy, 52_236_288, 10),
            cand(VmKind::Wsl, 2_579_218_432, 99),
        ];
        assert_eq!(select_vm_working_set(&candidates), Some(2_579_218_432));
    }

    #[test]
    fn select_falls_back_to_legacy_vmmem() {
        let candidates = [cand(VmKind::Legacy, 1_234, 7)];
        assert_eq!(select_vm_working_set(&candidates), Some(1_234));
    }

    #[test]
    fn select_returns_none_when_no_vm_present() {
        assert_eq!(select_vm_working_set(&[]), None);
    }

    #[test]
    fn select_cpu_prefers_wsl_over_legacy_vmmem() {
        let candidates = [
            cand(VmKind::Legacy, 10, 111),
            cand(VmKind::Wsl, 20, 222),
        ];
        assert_eq!(select_vm_cpu_ticks(&candidates), Some(222));
    }

    #[test]
    fn select_cpu_falls_back_to_legacy_vmmem() {
        let candidates = [cand(VmKind::Legacy, 10, 333)];
        assert_eq!(select_vm_cpu_ticks(&candidates), Some(333));
    }

    #[test]
    fn select_cpu_returns_none_when_no_vm_present() {
        assert_eq!(select_vm_cpu_ticks(&[]), None);
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib metrics::tests::select`
Expected: FAIL to compile — `VmCandidate` and `select_vm_cpu_ticks` do not exist yet.

- [ ] **Step 3: Introduce `VmCandidate`, the CPU-time reader, and the selectors**

In `src/metrics/mod.rs`, just after the `VmKind` enum, add:

```rust
/// A WSL VM host process found in the kernel snapshot.
#[derive(Debug, Clone, Copy)]
struct VmCandidate {
    kind: VmKind,
    /// Working-set (RSS) in bytes.
    working_set: u64,
    /// Cumulative CPU time (kernel + user) in 100 ns units.
    cpu_100ns: u64,
}
```

Replace `select_vm_working_set` with the `VmCandidate` form and add `select_vm_cpu_ticks` beside it:

```rust
/// Pick the WSL VM's working set from classified candidates, preferring the
/// modern `vmmemWSL` over the ambiguous legacy `vmmem`. `None` when neither is
/// present (the VM is not running).
fn select_vm_working_set(candidates: &[VmCandidate]) -> Option<u64> {
    select_vm(candidates).map(|c| c.working_set)
}

/// Pick the WSL VM's cumulative CPU time (100 ns units), with the same
/// `vmmemWSL`-over-`vmmem` preference. `None` when the VM is not running.
fn select_vm_cpu_ticks(candidates: &[VmCandidate]) -> Option<u64> {
    select_vm(candidates).map(|c| c.cpu_100ns)
}

/// The single WSL VM candidate, preferring `vmmemWSL` over legacy `vmmem`.
fn select_vm(candidates: &[VmCandidate]) -> Option<&VmCandidate> {
    let by_kind = |want: VmKind| candidates.iter().find(|c| c.kind == want);
    by_kind(VmKind::Wsl).or_else(|| by_kind(VmKind::Legacy))
}
```

Add the CPU-time reader near `unicode_string_lossy` (it is safe — it only reads a `[u8; 48]` array out of a borrowed record):

```rust
/// Sum of a process's kernel + user CPU time, in 100 ns units.
///
/// `windows-sys` 0.59 exposes the *documented* (winternl.h) form of
/// `SYSTEM_PROCESS_INFORMATION`, in which `WorkingSetSize` is a named field but
/// the CPU times are not — they live inside the `Reserved1: [u8; 48]` blob. That
/// blob is the stable (Vista+) sequence
/// `WorkingSetPrivateSize(8) HardFaultCount(4) NumberOfThreadsHighWatermark(4)`
/// `CycleTime(8) CreateTime(8) UserTime(8) KernelTime(8)`, so `UserTime` occupies
/// bytes 32..40 and `KernelTime` 40..48, each a little-endian `i64`. They are
/// summed (the order between the two is irrelevant to the sum) and clamped at 0.
fn process_cpu_100ns(
    info: &windows_sys::Win32::System::WindowsProgramming::SYSTEM_PROCESS_INFORMATION,
) -> u64 {
    let user = i64::from_le_bytes(info.Reserved1[32..40].try_into().unwrap());
    let kernel = i64::from_le_bytes(info.Reserved1[40..48].try_into().unwrap());
    user.saturating_add(kernel).max(0) as u64
}
```

In `enumerate_vm_candidates`, change the return type and the push:

```rust
fn enumerate_vm_candidates() -> Option<Vec<VmCandidate>> {
```

and inside the parse loop, where it currently pushes `(kind, info.WorkingSetSize as u64)`:

```rust
                    if let Some(kind) = classify_vm(&name) {
                        candidates.push(VmCandidate {
                            kind,
                            working_set: info.WorkingSetSize as u64,
                            cpu_100ns: process_cpu_100ns(info),
                        });
                    }
```

- [ ] **Step 4: Populate the real CPU fields in `sample()`**

Replace the body of `sample()` with:

```rust
pub fn sample() -> Option<MetricsSample> {
    let candidates = enumerate_vm_candidates()?;
    let taken_at = Some(Instant::now());
    let logical_cpus = std::thread::available_parallelism()
        .map(|n| n.get() as u32)
        .unwrap_or(1);

    let mut system = System::new();
    system.refresh_memory();

    Some(MetricsSample {
        vmmem_bytes: select_vm_working_set(&candidates),
        total_mem_bytes: system.total_memory(),
        vm_cpu_100ns: select_vm_cpu_ticks(&candidates),
        taken_at,
        logical_cpus,
    })
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --lib metrics::tests`
Expected: PASS (all `select_*` and `select_cpu_*` tests green).

- [ ] **Step 6: Commit**

```bash
git add src/metrics/mod.rs
git commit -m "feat(metrics): read vmmemWSL CPU time from the process snapshot"
```

---

## Task 3: Compute CPU% in `MetricsHistory::push`

Carry a previous (ticks, timestamp) pair and turn consecutive samples into a clamped 0–100% figure plus a CPU sparkline buffer — all pure arithmetic, fully unit-tested.

**Files:**
- Modify: `src/metrics/mod.rs` (`MetricsHistory` struct, `new`, `push`, add `push_cpu` + `cpu_sparkline`, tests)

- [ ] **Step 1: Write the failing CPU-math tests**

In `src/metrics/mod.rs` `mod tests`, add:

```rust
    #[test]
    fn computes_cpu_pct_from_consecutive_samples() {
        use std::time::Duration;
        let mut history = MetricsHistory::new(5);
        let t0 = Instant::now();
        // First sample primes the previous reading; no percentage yet.
        history.push(&MetricsSample {
            vm_cpu_100ns: Some(0),
            taken_at: Some(t0),
            logical_cpus: 4,
            ..Default::default()
        });
        assert_eq!(history.latest_vm_cpu_pct, None);
        // 1 s later the VM has burned 2.0 CPU-seconds (= 2.0e7 in 100 ns units)
        // across 4 logical CPUs ⇒ 2 / (1 × 4) × 100 = 50%.
        history.push(&MetricsSample {
            vm_cpu_100ns: Some(20_000_000),
            taken_at: Some(t0 + Duration::from_secs(1)),
            logical_cpus: 4,
            ..Default::default()
        });
        let pct = history.latest_vm_cpu_pct.expect("pct after second sample");
        assert!((pct - 50.0).abs() < 0.01, "expected ~50%, got {pct}");
    }

    #[test]
    fn cpu_pct_is_clamped_to_100() {
        use std::time::Duration;
        let mut history = MetricsHistory::new(5);
        let t0 = Instant::now();
        history.push(&MetricsSample {
            vm_cpu_100ns: Some(0),
            taken_at: Some(t0),
            logical_cpus: 2,
            ..Default::default()
        });
        // 100 CPU-seconds in 1 s across 2 CPUs would be 5000% ⇒ clamped to 100.
        history.push(&MetricsSample {
            vm_cpu_100ns: Some(1_000_000_000),
            taken_at: Some(t0 + Duration::from_secs(1)),
            logical_cpus: 2,
            ..Default::default()
        });
        assert_eq!(history.latest_vm_cpu_pct, Some(100.0));
    }

    #[test]
    fn stopped_vm_clears_cpu_and_resets_delta() {
        use std::time::Duration;
        let mut history = MetricsHistory::new(5);
        let t0 = Instant::now();
        history.push(&MetricsSample {
            vm_cpu_100ns: Some(0),
            taken_at: Some(t0),
            logical_cpus: 4,
            ..Default::default()
        });
        history.push(&MetricsSample {
            vm_cpu_100ns: Some(20_000_000),
            taken_at: Some(t0 + Duration::from_secs(1)),
            logical_cpus: 4,
            ..Default::default()
        });
        assert!(history.latest_vm_cpu_pct.is_some());
        // VM stops: percentage clears and the sparkline records a 0 dip.
        history.push(&MetricsSample {
            vm_cpu_100ns: None,
            taken_at: Some(t0 + Duration::from_secs(2)),
            logical_cpus: 4,
            ..Default::default()
        });
        assert_eq!(history.latest_vm_cpu_pct, None);
        assert_eq!(*history.cpu_sparkline().last().unwrap(), 0);
        // Restart: the next single sample must NOT diff against the pre-stop
        // counter (which would compute a bogus huge %); the delta was reset.
        history.push(&MetricsSample {
            vm_cpu_100ns: Some(999_999_999),
            taken_at: Some(t0 + Duration::from_secs(3)),
            logical_cpus: 4,
            ..Default::default()
        });
        assert_eq!(history.latest_vm_cpu_pct, None);
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib metrics::tests::computes_cpu_pct_from_consecutive_samples`
Expected: FAIL to compile — `latest_vm_cpu_pct` and `cpu_sparkline` do not exist yet.

- [ ] **Step 3: Extend `MetricsHistory` and implement the CPU math**

In `src/metrics/mod.rs`, replace the `MetricsHistory` struct with:

```rust
/// A fixed-capacity ring buffer of recent samples for sparkline rendering.
#[derive(Debug, Clone)]
pub struct MetricsHistory {
    capacity: usize,
    vmmem: VecDeque<u64>,
    /// CPU sparkline buffer, stored as `pct × 10` (0..=1000) for finer bars.
    vm_cpu: VecDeque<u64>,
    /// The most recent vmmem reading (None if the VM is not running).
    pub latest_vmmem: Option<u64>,
    /// The most recent VM CPU percentage (0..=100), or `None` until a second
    /// sample exists or while the VM is stopped.
    pub latest_vm_cpu_pct: Option<f32>,
    /// Total physical memory, in bytes.
    pub total_mem_bytes: u64,
    /// Previous CPU reading `(cumulative_100ns, taken_at)` for the delta.
    prev_cpu: Option<(u64, Instant)>,
}
```

Replace `MetricsHistory::new` with:

```rust
    /// Create a history retaining up to `capacity` samples.
    pub fn new(capacity: usize) -> Self {
        let capacity = capacity.max(1);
        Self {
            capacity,
            vmmem: VecDeque::with_capacity(capacity),
            vm_cpu: VecDeque::with_capacity(capacity),
            latest_vmmem: None,
            latest_vm_cpu_pct: None,
            total_mem_bytes: 0,
            prev_cpu: None,
        }
    }
```

Replace `push` and add `push_cpu` + `cpu_sparkline` (keep the existing `sparkline` and `is_empty`):

```rust
    /// Record a new sample, dropping the oldest if at capacity. A "VM stopped"
    /// sample (`None`) is recorded as zero so the sparkline shows the dip.
    pub fn push(&mut self, sample: &MetricsSample) {
        self.latest_vmmem = sample.vmmem_bytes;
        if sample.total_mem_bytes > 0 {
            self.total_mem_bytes = sample.total_mem_bytes;
        }
        self.vmmem.push_back(sample.vmmem_bytes.unwrap_or(0));
        while self.vmmem.len() > self.capacity {
            self.vmmem.pop_front();
        }
        self.push_cpu(sample);
    }

    /// Derive the VM CPU percentage from the delta against the previous sample
    /// and append it to the CPU sparkline. Pure arithmetic over the timestamps
    /// and tick counts handed in — no clock reads here.
    fn push_cpu(&mut self, sample: &MetricsSample) {
        let last = self.latest_vm_cpu_pct;
        let pct = match (sample.vm_cpu_100ns, sample.taken_at) {
            (Some(curr_ticks), Some(now)) => {
                let cpus = sample.logical_cpus.max(1) as f64;
                let computed = match self.prev_cpu {
                    Some((prev_ticks, prev_t)) => {
                        let dt = now.saturating_duration_since(prev_t).as_secs_f64();
                        if dt > 0.0 {
                            // 100 ns units → seconds of CPU busy time.
                            let busy = curr_ticks.saturating_sub(prev_ticks) as f64 / 1.0e7;
                            Some(((busy / (dt * cpus)) * 100.0).clamp(0.0, 100.0) as f32)
                        } else {
                            last // two samples at the same instant: keep last
                        }
                    }
                    None => None, // first sample since start/restart: no delta yet
                };
                self.prev_cpu = Some((curr_ticks, now));
                computed
            }
            _ => {
                // VM stopped (or a timing-less synthetic sample): reset so a
                // restart does not diff against a stale counter.
                self.prev_cpu = None;
                None
            }
        };
        self.latest_vm_cpu_pct = pct;
        let bar = pct.map(|p| (p * 10.0).round() as u64).unwrap_or(0);
        self.vm_cpu.push_back(bar);
        while self.vm_cpu.len() > self.capacity {
            self.vm_cpu.pop_front();
        }
    }

    /// The CPU history as a vec for `Sparkline::data` (values are `pct × 10`).
    pub fn cpu_sparkline(&self) -> Vec<u64> {
        self.vm_cpu.iter().copied().collect()
    }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --lib metrics::tests`
Expected: PASS (the three CPU tests plus all existing ones).

- [ ] **Step 5: Commit**

```bash
git add src/metrics/mod.rs
git commit -m "feat(metrics): derive VM CPU% from consecutive samples"
```

---

## Task 4: i18n keys for the CPU line and trend

**Files:**
- Modify: `src/i18n/mod.rs` (`Key` enum, `Key::ALL`, `entry()` match)

- [ ] **Step 1: Add the enum variants**

In `src/i18n/mod.rs`, in the `Key` enum's `// Detail pane.` group, after `DetailVmMemTrend,` add:

```rust
    DetailVmCpu,
    DetailVmCpuTrend,
```

- [ ] **Step 2: Add them to `Key::ALL`**

In the `Key::ALL` slice, after `Key::DetailVmMemTrend,` add:

```rust
        Key::DetailVmCpu,
        Key::DetailVmCpuTrend,
```

- [ ] **Step 3: Add the catalog entries and relabel the memory trend**

In the `entry()` match, replace the `DetailVmMemTrend` arm and add the two new arms:

```rust
        Key::DetailVmMemTrend => ("Mem", "メモリ推移"),
        Key::DetailVmCpu => ("VM CPU", "VM CPU"),
        Key::DetailVmCpuTrend => ("CPU", "CPU推移"),
```

- [ ] **Step 4: Run the catalog completeness test**

Run: `cargo test --lib i18n`
Expected: PASS — `every_key_has_both_languages` covers the two new keys (both languages are filled in) and confirms `Key::ALL` is in sync.

- [ ] **Step 5: Commit**

```bash
git add src/i18n/mod.rs
git commit -m "i18n: add VM CPU + CPU-trend keys; relabel memory trend"
```

---

## Task 5: Render the CPU line and trend sparkline

Add the `VM CPU` info line, a second labeled sparkline row via a shared helper, grow the detail pane so both trends fit, and guard against tiny terminals.

**Files:**
- Modify: `src/ui/mod.rs` (`view()` height, `render_detail`, new `render_sparkline_row` helper, tests)

- [ ] **Step 1: Write the failing render tests**

In `src/ui/mod.rs` `mod tests`, extend `renders_detail_pane_with_vm_memory` to also push a CPU-bearing pair and assert the CPU line, and add a short-height no-panic test. Replace `renders_detail_pane_with_vm_memory` with:

```rust
    #[test]
    fn renders_detail_pane_with_vm_memory_and_cpu() {
        use crate::metrics::MetricsSample;
        use std::time::{Duration, Instant};
        let mut model = sample();
        let t0 = Instant::now();
        model.metrics.push(&MetricsSample {
            vmmem_bytes: Some(2 * 1024 * 1024 * 1024),
            total_mem_bytes: 8 * 1024 * 1024 * 1024,
            vm_cpu_100ns: Some(0),
            taken_at: Some(t0),
            logical_cpus: 4,
        });
        // Second sample 1 s later: 2.0 CPU-seconds / (1 s × 4) = 50%.
        model.metrics.push(&MetricsSample {
            vmmem_bytes: Some(2 * 1024 * 1024 * 1024),
            total_mem_bytes: 8 * 1024 * 1024 * 1024,
            vm_cpu_100ns: Some(20_000_000),
            taken_at: Some(t0 + Duration::from_secs(1)),
            logical_cpus: 4,
        });
        let rendered = render(&model, 110, 24);
        assert!(rendered.contains("Detail: Debian"), "detail title missing");
        assert!(rendered.contains("VM Mem"), "vm memory line missing");
        assert!(rendered.contains("2.0 GB"), "vm memory value missing");
        assert!(rendered.contains("VM CPU"), "vm cpu line missing");
        assert!(rendered.contains("50.0 %"), "vm cpu value missing");
        assert!(rendered.contains("Mem"), "memory trend label missing");
        assert!(rendered.contains("CPU"), "cpu trend label missing");
    }

    #[test]
    fn detail_pane_survives_tiny_height() {
        use crate::metrics::MetricsSample;
        let mut model = sample();
        model.metrics.push(&MetricsSample {
            vmmem_bytes: Some(1024),
            total_mem_bytes: 2048,
            ..Default::default()
        });
        // A 6-row terminal squeezes the detail interior below two trend rows;
        // rendering must not panic.
        let _ = render(&model, 80, 6);
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib ui::tests::renders_detail_pane_with_vm_memory_and_cpu`
Expected: FAIL — the `VM CPU` line and `50.0 %` value are not rendered yet.

- [ ] **Step 3: Grow the detail pane**

In `src/ui/mod.rs` `view()`, change the middle constraint from `Constraint::Length(9)` to:

```rust
    let chunks = Layout::vertical([
        Constraint::Min(5),
        Constraint::Length(11),
        Constraint::Length(1),
    ])
    .split(area);
```

- [ ] **Step 4: Add the CPU info line and the second trend row**

In `src/ui/mod.rs`, rewrite `render_detail`'s body from the `rows` split onward (keep the block/title/`selected_distro` guard and the `path`/`disk`/`default` computations above it exactly as they are):

```rust
    // Bottom of the pane: a memory trend row and a CPU trend row, pinned below
    // the info block. Tiny terminals (interior < 3 rows) drop the CPU row.
    let two_trends = inner.height >= 3;
    let rows = if two_trends {
        Layout::vertical([
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner)
    } else {
        Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(inner)
    };

    let mut info = format!(
        "{}: {}\n{}: {}    {}: {}\n{}: {}\n{}: {}\n{}: {}\n{}: {}",
        t(lang, Key::DetailState),
        state_label(lang, distro.state),
        t(lang, Key::DetailVersion),
        distro.version,
        t(lang, Key::DetailDefault),
        default,
        t(lang, Key::DetailDisk),
        disk,
        t(lang, Key::DetailPath),
        path,
        t(lang, Key::DetailVmMem),
        vm_mem_line(lang, &model.metrics, model.vm_mem_total),
    );
    info.push_str(&format!(
        "\n{}: {}",
        t(lang, Key::DetailVmCpu),
        vm_cpu_line(lang, &model.metrics),
    ));
    if let Some((used, total)) = distro.inner_disk {
        info.push_str(&format!(
            "\n{}: {} / {}",
            t(lang, Key::DetailInnerDisk),
            human_size(used),
            human_size(total)
        ));
    }
    f.render_widget(Paragraph::new(info), rows[0]);

    render_sparkline_row(
        f,
        rows[1],
        t(lang, Key::DetailVmMemTrend),
        &model.metrics.sparkline(),
    );
    if two_trends {
        render_sparkline_row(
            f,
            rows[2],
            t(lang, Key::DetailVmCpuTrend),
            &model.metrics.cpu_sparkline(),
        );
    }
}
```

(The original trailing block that built `trend`/`sparkline` inline is fully replaced by the two `render_sparkline_row` calls above.)

Immediately after `render_detail`, add the helper and the CPU-line formatter:

```rust
/// Render a short label followed by a sparkline filling the rest of `area`.
/// Label width is measured in display columns so CJK glyphs line up.
fn render_sparkline_row(f: &mut Frame, area: Rect, label: &str, data: &[u64]) {
    let label_cols = UnicodeWidthStr::width(label) as u16 + 1;
    let cols =
        Layout::horizontal([Constraint::Length(label_cols), Constraint::Min(1)]).split(area);
    f.render_widget(Paragraph::new(label), cols[0]);
    let sparkline = Sparkline::default()
        .data(data)
        .style(Style::default().fg(Color::Cyan));
    f.render_widget(sparkline, cols[1]);
}

/// The `VM CPU` value line: `NN.N %`, or `—` when no percentage is available
/// yet (first sample) or the VM is stopped.
fn vm_cpu_line(lang: Lang, metrics: &MetricsHistory) -> String {
    match metrics.latest_vm_cpu_pct {
        Some(pct) => format!("{pct:.1} %"),
        None => t(lang, Key::VmNotRunning).to_string(),
    }
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --lib ui::tests`
Expected: PASS (the new CPU render test and the tiny-height test, plus all existing UI tests).

- [ ] **Step 6: Commit**

```bash
git add src/ui/mod.rs
git commit -m "feat(ui): show VM CPU line and a dedicated CPU trend sparkline"
```

---

## Task 6: Real-machine smoke test (ignored by default)

**Files:**
- Modify: `src/metrics/mod.rs` (one `#[ignore]` test)

- [ ] **Step 1: Add the ignored monotonic-CPU test**

In `src/metrics/mod.rs` `mod tests`, beside the other `#[ignore]` tests, add:

```rust
    #[test]
    #[ignore = "requires a running WSL distro on the host"]
    fn sample_reports_monotonic_vm_cpu_time() {
        // Cumulative CPU time only ever increases while the VM runs, so a second
        // snapshot must be ≥ the first.
        let first = sample().expect("process query should succeed");
        let a = first.vm_cpu_100ns.expect("vmmemWSL should be running");
        std::thread::sleep(std::time::Duration::from_millis(200));
        let second = sample().expect("process query should succeed");
        let b = second.vm_cpu_100ns.expect("vmmemWSL should be running");
        assert!(b >= a, "cumulative CPU time went backwards: {a} -> {b}");
    }
```

- [ ] **Step 2: Confirm it compiles and is skipped by default**

Run: `cargo test --lib metrics::tests::sample_reports_monotonic_vm_cpu_time`
Expected: the test is listed and reported as `ignored` (0 run). It compiles.

- [ ] **Step 3: Commit**

```bash
git add src/metrics/mod.rs
git commit -m "test(metrics): ignored smoke test for monotonic VM CPU time"
```

---

## Task 7: Full CI gate

**Files:** none (verification only)

- [ ] **Step 1: Format check**

Run: `cargo fmt --all -- --check`
Expected: PASS (no diff). If it fails, run `cargo fmt --all` and re-commit.

- [ ] **Step 2: Lint (warnings are errors)**

Run: `cargo clippy --all-targets -- -D warnings`
Expected: PASS. Likely nits to preempt: prefer `c.kind == want` over pattern binds; ensure no unused imports; `as f64`/`as f32` casts are intentional (CPU math) — add `#[allow(clippy::cast_precision_loss)]` only if clippy flags them and there is no cleaner form.

- [ ] **Step 3: Full test suite**

Run: `cargo test --all`
Expected: PASS — all prior tests plus the new metrics/ui tests; ignored tests skipped.

- [ ] **Step 4: Manual smoke (optional, on a real WSL host)**

Run: `cargo run`
Expected: the detail pane shows a `VM CPU: NN.N %` line that moves between polls and a second cyan sparkline labeled `CPU`/`CPU推移`; a stopped distro's VM CPU shows `—`.

- [ ] **Step 5: Final commit if formatting changed**

```bash
git add -A
git commit -m "style: cargo fmt"
```

(Skip if Steps 1–3 produced no changes.)

---

## Self-review notes (for the implementer)

- **Spec coverage:** §3 CPU computation → Tasks 2–3; §4.1 metrics → Tasks 1–3; §4.2 reducer untouched → confirmed (no `src/app/` changes); §4.3 UI → Task 5; §4.4 i18n → Task 4; §5 edge cases → Task 3 tests (first/stopped/clamp) + Task 5 `—` formatting; §6 testing → Tasks 2,3,5,6.
- **Type consistency:** `VmCandidate { kind, working_set, cpu_100ns }`, `select_vm_working_set`/`select_vm_cpu_ticks`/`select_vm`, `process_cpu_100ns`, `MetricsHistory.latest_vm_cpu_pct`/`cpu_sparkline`/`push_cpu`/`prev_cpu`, `render_sparkline_row`, `vm_cpu_line`, keys `DetailVmCpu`/`DetailVmCpuTrend` — all names used consistently across tasks.
- **No placeholders:** every code step shows the full code; no TODO/TBD.
