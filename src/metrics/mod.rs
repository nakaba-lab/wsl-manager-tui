//! Metrics: sampling of `vmmemWSL` memory plus a ring-buffer history for
//! sparklines. The WSL2 VM is shared by all distributions, so VM memory is a
//! single machine-wide figure (surfaced as such in the UI). No UI here.

use std::collections::VecDeque;
use std::path::Path;

use sysinfo::{ProcessesToUpdate, System};

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

/// Sample current WSL VM memory usage. Builds a one-shot `System` view and only
/// refreshes memory and processes (not CPU/disks). Blocking; callers run it off
/// the async loop.
pub fn sample() -> MetricsSample {
    let mut system = System::new();
    system.refresh_memory();
    system.refresh_processes(ProcessesToUpdate::All, true);

    let vmmem_bytes = system
        .processes()
        .values()
        .find(|process| is_vmmem(&process.name().to_string_lossy()))
        .map(|process| process.memory());

    MetricsSample {
        vmmem_bytes,
        total_mem_bytes: system.total_memory(),
    }
}

/// The size of a file on disk (e.g. a distro's `ext4.vhdx`), in bytes. Returns
/// `None` if the path cannot be read.
pub fn disk_size(path: &Path) -> Option<u64> {
    std::fs::metadata(path).ok().map(|meta| meta.len())
}

/// Whether a process name is the WSL VM. Handles both the current `vmmemWSL`
/// and the older `vmmem`, with or without an `.exe` suffix.
fn is_vmmem(name: &str) -> bool {
    let name = name.strip_suffix(".exe").unwrap_or(name);
    name.eq_ignore_ascii_case("vmmemWSL") || name.eq_ignore_ascii_case("vmmem")
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
    fn is_vmmem_matches_variants() {
        assert!(is_vmmem("vmmemWSL"));
        assert!(is_vmmem("vmmem"));
        assert!(is_vmmem("vmmemWSL.exe"));
        assert!(is_vmmem("VMMEMWSL"));
        assert!(!is_vmmem("explorer.exe"));
        assert!(!is_vmmem("memcached"));
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
        let sample = sample();
        assert!(sample.total_mem_bytes > 0);
    }
}
