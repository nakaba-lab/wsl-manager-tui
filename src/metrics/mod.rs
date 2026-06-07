//! Metrics: sampling of `vmmemWSL` memory plus a ring-buffer history for
//! sparklines. The WSL2 VM is shared by all distributions, so VM memory is a
//! single machine-wide figure (surfaced as such in the UI). No UI here.

use std::collections::VecDeque;
use std::path::Path;

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

    let mut system = System::new();
    system.refresh_memory();

    Some(MetricsSample {
        vmmem_bytes: select_vm_working_set(&candidates),
        total_mem_bytes: system.total_memory(),
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
fn select_vm_working_set(candidates: &[(VmKind, u64)]) -> Option<u64> {
    let by_kind = |want: VmKind| {
        candidates
            .iter()
            .find(|(kind, _)| *kind == want)
            .map(|(_, bytes)| *bytes)
    };
    by_kind(VmKind::Wsl).or_else(|| by_kind(VmKind::Legacy))
}

/// Enumerate WSL VM host processes as `(kind, working_set_bytes)` from the
/// kernel's process snapshot. `None` means the query failed (so the caller
/// should keep its previous reading); `Some(vec)` is authoritative, and an
/// empty vec means no VM process is running.
///
/// This deliberately bypasses sysinfo: `vmmemWSL` is a protected *minimal
/// process* that cannot be opened even with `PROCESS_QUERY_LIMITED_INFORMATION`,
/// so every handle-based memory reader (sysinfo's `process.memory()`,
/// `GetProcessMemoryInfo`) reports 0 for it — the cause of the "VM memory: 0 B"
/// bug. `NtQuerySystemInformation(SystemProcessInformation)` — the same source
/// Task Manager uses — carries each process's `WorkingSetSize` without a handle
/// and needs no elevation.
fn enumerate_vm_candidates() -> Option<Vec<(VmKind, u64)>> {
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
                        candidates.push((kind, info.WorkingSetSize as u64));
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
    /// The most recent vmmem reading (None if the VM is not running).
    pub latest_vmmem: Option<u64>,
    /// Total physical memory, in bytes.
    pub total_mem_bytes: u64,
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
            latest_vmmem: None,
            total_mem_bytes: 0,
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

    #[test]
    fn select_prefers_wsl_over_legacy_vmmem() {
        // A machine running WSL2 *and* another Hyper-V VM has both processes;
        // the legacy bare `vmmem` is the *other* VM, so `vmmemWSL` must win
        // regardless of enumeration order.
        let candidates = [(VmKind::Legacy, 52_236_288), (VmKind::Wsl, 2_579_218_432)];
        assert_eq!(select_vm_working_set(&candidates), Some(2_579_218_432));
    }

    #[test]
    fn select_falls_back_to_legacy_vmmem() {
        // Older WSL builds named the VM process bare `vmmem`; with no
        // `vmmemWSL` present it is the WSL VM.
        let candidates = [(VmKind::Legacy, 1_234)];
        assert_eq!(select_vm_working_set(&candidates), Some(1_234));
    }

    #[test]
    fn select_returns_none_when_no_vm_present() {
        assert_eq!(select_vm_working_set(&[]), None);
    }

    #[test]
    fn history_is_a_ring_buffer() {
        let mut history = MetricsHistory::new(3);
        for value in [10, 20, 30, 40] {
            history.push(&MetricsSample {
                vmmem_bytes: Some(value),
                total_mem_bytes: 100,
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
        });
        assert_eq!(history.sparkline(), vec![0]);
        assert_eq!(history.latest_vmmem, None);
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
}
