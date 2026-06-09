//! Metrics: sampling of `vmmemWSL` memory plus a ring-buffer history for
//! sparklines. The WSL2 VM is shared by all distributions, so VM memory is a
//! single machine-wide figure (surfaced as such in the UI). No UI here.

use std::collections::VecDeque;
use std::path::Path;
use std::time::Instant;

use sysinfo::System;

/// Default number of samples retained for the sparkline.
const DEFAULT_HISTORY: usize = 60;

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

/// Sample current WSL VM memory usage. Reads the VM working set from the kernel
/// process snapshot (see [`enumerate_vm_candidates`]) and total physical memory
/// from a one-shot `System` view. Blocking; callers run it off the async loop.
///
/// Returns `None` when the kernel process query fails, so the caller can keep
/// the previous reading rather than flip the display to "VM not running" on a
/// transient hiccup. A successful query with no VM process yields
/// `Some(sample)` whose `vmmem_bytes` is `None` (the VM is genuinely stopped).
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

/// The size of a file on disk (e.g. a distro's `ext4.vhdx`), in bytes. Returns
/// `None` if the path cannot be read.
pub fn disk_size(path: &Path) -> Option<u64> {
    std::fs::metadata(path).ok().map(|meta| meta.len())
}

/// Which WSL VM host process a name refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VmKind {
    /// Modern WSL2 VM, unambiguously named `vmmemWSL`.
    Wsl,
    /// Legacy bare `vmmem`. Older WSL builds named the VM this; on a machine
    /// also running other Hyper-V VMs (Sandbox, WSA, …) it may be a *different*
    /// VM, so it is only used as a fallback (see [`select_vm_working_set`]).
    Legacy,
}

/// A WSL VM host process found in the kernel snapshot.
#[derive(Debug, Clone, Copy)]
struct VmCandidate {
    kind: VmKind,
    /// Working-set (RSS) in bytes.
    working_set: u64,
    /// Cumulative CPU time (kernel + user) in 100 ns units.
    cpu_100ns: u64,
}

/// Classify a process name as the WSL2 VM, the legacy `vmmem`, or neither.
/// Case- and `.exe`-insensitive.
fn classify_vm(name: &str) -> Option<VmKind> {
    let name = name.strip_suffix(".exe").unwrap_or(name);
    if name.eq_ignore_ascii_case("vmmemWSL") {
        Some(VmKind::Wsl)
    } else if name.eq_ignore_ascii_case("vmmem") {
        Some(VmKind::Legacy)
    } else {
        None
    }
}

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

/// Enumerate WSL VM host processes from the kernel's process snapshot. `None`
/// means the query failed (so the caller should keep its previous reading);
/// `Some(vec)` is authoritative, and an empty vec means no VM process is running.
///
/// This deliberately bypasses sysinfo: `vmmemWSL` is a protected *minimal
/// process* that cannot be opened even with `PROCESS_QUERY_LIMITED_INFORMATION`,
/// so every handle-based memory reader (sysinfo's `process.memory()`,
/// `GetProcessMemoryInfo`) reports 0 for it — the cause of the "VM memory: 0 B"
/// bug. `NtQuerySystemInformation(SystemProcessInformation)` — the same source
/// Task Manager uses — carries each process's `WorkingSetSize` without a handle
/// and needs no elevation.
fn enumerate_vm_candidates() -> Option<Vec<VmCandidate>> {
    use windows_sys::Wdk::System::SystemInformation::{
        NtQuerySystemInformation, SystemProcessInformation,
    };
    use windows_sys::Win32::System::WindowsProgramming::SYSTEM_PROCESS_INFORMATION;

    // The buffer was too small for the snapshot; `return_len` holds the size now
    // needed, so grow and retry.
    const STATUS_INFO_LENGTH_MISMATCH: i32 = 0xC000_0004u32 as i32;

    // `SYSTEM_PROCESS_INFORMATION` holds pointers, so the buffer must be
    // pointer-aligned; a `Vec<u64>` guarantees 8-byte alignment. The snapshot is
    // ~1 MiB on a typical host, so start at 2 MiB to usually succeed first try.
    let mut buf: Vec<u64> = vec![0; 256 * 1024];

    for _ in 0..8 {
        let mut return_len: u32 = 0;
        let byte_len = (buf.len() * std::mem::size_of::<u64>()) as u32;
        // SAFETY: `buf` is a live, writable allocation of exactly `byte_len`
        // bytes; `return_len` is a caller-owned `u32`. ntdll writes only within
        // `byte_len` and reports the required size in `return_len`.
        let status = unsafe {
            NtQuerySystemInformation(
                SystemProcessInformation,
                buf.as_mut_ptr().cast(),
                byte_len,
                &mut return_len,
            )
        };

        if status == STATUS_INFO_LENGTH_MISMATCH {
            let needed = return_len as usize / std::mem::size_of::<u64>() + 1024;
            buf.resize(needed.max(buf.len() * 2), 0);
            continue;
        }
        if status < 0 {
            return None; // query failed; let the caller keep its last reading
        }

        let mut candidates = Vec::new();
        // SAFETY: on success ntdll wrote a chain of `SYSTEM_PROCESS_INFORMATION`
        // records into `buf`, each `NextEntryOffset` bytes after the previous and
        // terminated by a zero offset, all within the validated region.
        unsafe {
            let mut cursor = buf.as_ptr().cast::<u8>();
            loop {
                let info = &*cursor.cast::<SYSTEM_PROCESS_INFORMATION>();
                if let Some(name) = unicode_string_lossy(&info.ImageName) {
                    if let Some(kind) = classify_vm(&name) {
                        candidates.push(VmCandidate {
                            kind,
                            working_set: info.WorkingSetSize as u64,
                            cpu_100ns: process_cpu_100ns(info),
                        });
                    }
                }
                if info.NextEntryOffset == 0 {
                    break;
                }
                cursor = cursor.add(info.NextEntryOffset as usize);
            }
        }
        return Some(candidates);
    }

    None // exhausted retries without a successful query
}

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

/// Decode a kernel `UNICODE_STRING` image name into an owned string, or `None`
/// when empty.
///
/// # Safety
/// `s.Buffer` must be null or point to at least `s.Length` bytes of UTF-16.
unsafe fn unicode_string_lossy(
    s: &windows_sys::Win32::Foundation::UNICODE_STRING,
) -> Option<String> {
    if s.Buffer.is_null() || s.Length == 0 {
        return None;
    }
    let units = s.Length as usize / 2;
    let slice = std::slice::from_raw_parts(s.Buffer, units);
    Some(String::from_utf16_lossy(slice))
}

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

impl Default for MetricsHistory {
    fn default() -> Self {
        Self::new(DEFAULT_HISTORY)
    }
}

impl MetricsHistory {
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
        // Snapshot before mutation; used to freeze the display when two samples
        // land at the same instant (dt == 0).
        let prev_pct = self.latest_vm_cpu_pct;
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
                            prev_pct // two samples at the same instant: keep last
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

    /// The vmmem history as a vec for `Sparkline::data`.
    pub fn sparkline(&self) -> Vec<u64> {
        self.vmmem.iter().copied().collect()
    }

    /// Whether any samples have been recorded.
    pub fn is_empty(&self) -> bool {
        self.vmmem.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_vm_distinguishes_wsl_from_legacy() {
        assert_eq!(classify_vm("vmmemWSL"), Some(VmKind::Wsl));
        assert_eq!(classify_vm("vmmemWSL.exe"), Some(VmKind::Wsl));
        assert_eq!(classify_vm("VMMEMWSL"), Some(VmKind::Wsl));
        assert_eq!(classify_vm("vmmem"), Some(VmKind::Legacy));
        assert_eq!(classify_vm("vmmem.exe"), Some(VmKind::Legacy));
        assert_eq!(classify_vm("explorer.exe"), None);
        assert_eq!(classify_vm("memcached"), None);
    }

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
        let candidates = [cand(VmKind::Legacy, 10, 111), cand(VmKind::Wsl, 20, 222)];
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

    #[test]
    fn history_is_a_ring_buffer() {
        let mut history = MetricsHistory::new(3);
        for value in [10, 20, 30, 40] {
            history.push(&MetricsSample {
                vmmem_bytes: Some(value),
                total_mem_bytes: 100,
                ..Default::default()
            });
        }
        assert_eq!(history.sparkline(), vec![20, 30, 40]);
        assert_eq!(history.latest_vmmem, Some(40));
        assert_eq!(history.total_mem_bytes, 100);
    }

    #[test]
    fn history_records_stopped_vm_as_zero() {
        let mut history = MetricsHistory::new(5);
        history.push(&MetricsSample {
            vmmem_bytes: None,
            total_mem_bytes: 100,
            ..Default::default()
        });
        assert_eq!(history.sparkline(), vec![0]);
        assert_eq!(history.latest_vmmem, None);
    }

    #[test]
    fn process_cpu_100ns_sums_user_and_kernel_excluding_create_time() {
        use windows_sys::Win32::System::WindowsProgramming::SYSTEM_PROCESS_INFORMATION;
        // The struct holds raw pointers; a zeroed value is valid (null/0) and we
        // only read the `Reserved1` byte blob.
        let mut info: SYSTEM_PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
        // CreateTime (bytes 24..32) MUST be ignored: poison it with a sentinel
        // that would blow the result up if the offsets ever drifted to include it.
        info.Reserved1[24..32].copy_from_slice(&i64::MAX.to_le_bytes());
        info.Reserved1[32..40].copy_from_slice(&5_000_000i64.to_le_bytes()); // UserTime
        info.Reserved1[40..48].copy_from_slice(&3_000_000i64.to_le_bytes()); // KernelTime
        assert_eq!(process_cpu_100ns(&info), 8_000_000);
    }

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

    #[test]
    fn cpu_pct_unchanged_on_same_instant_sample() {
        use std::time::Duration;
        let mut history = MetricsHistory::new(5);
        let t0 = Instant::now();
        history.push(&MetricsSample {
            vm_cpu_100ns: Some(0),
            taken_at: Some(t0),
            logical_cpus: 4,
            ..Default::default()
        });
        let t1 = t0 + Duration::from_secs(1);
        history.push(&MetricsSample {
            vm_cpu_100ns: Some(20_000_000),
            taken_at: Some(t1),
            logical_cpus: 4,
            ..Default::default()
        });
        let pct = history.latest_vm_cpu_pct.expect("pct after second sample");
        // A third sample at the SAME instant (dt == 0) must keep the previous
        // pct rather than divide by zero or reset.
        history.push(&MetricsSample {
            vm_cpu_100ns: Some(50_000_000),
            taken_at: Some(t1),
            logical_cpus: 4,
            ..Default::default()
        });
        assert_eq!(history.latest_vm_cpu_pct, Some(pct));
    }

    #[test]
    #[ignore = "samples the real machine"]
    fn sample_reads_total_memory() {
        let sample = sample().expect("process query should succeed");
        assert!(sample.total_mem_bytes > 0);
    }

    #[test]
    #[ignore = "requires a running WSL distro on the host"]
    fn sample_reports_running_wsl_vm_memory() {
        // With a WSL distro running, the shared VM (`vmmemWSL`) holds a working
        // set well above any plausible idle reading. A handle-based reader
        // (sysinfo) returns 0 here; the NtQuerySystemInformation path does not.
        let sample = sample().expect("process query should succeed");
        let bytes = sample.vmmem_bytes.expect("vmmemWSL should be running");
        assert!(
            bytes > 64 * 1024 * 1024,
            "vmmem working set unexpectedly small: {bytes} bytes"
        );
    }

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
}
