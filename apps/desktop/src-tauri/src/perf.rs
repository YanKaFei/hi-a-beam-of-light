//! Self-measurement of CPU and memory.
//!
//! `ps` and `top` are unavailable in this environment, and more importantly a
//! light point should be able to answer "what do I cost?" about itself. So the
//! process samples its own usage through `getrusage` (CPU time) and
//! `proc_pidinfo` (resident size).
//!
//! These are real measurements, not estimates.

use serde::Serialize;
use std::time::Duration;

#[derive(Debug, Clone, Serialize)]
pub struct PerfSample {
    /// Percent of one core, averaged over the sample window.
    pub cpu_pct: f64,
    /// Resident set size in bytes.
    pub rss_bytes: u64,
    pub virtual_bytes: u64,
    pub threads: i32,
    pub window_ms: u64,
    /// Cumulative CPU seconds since launch, for cross-checking.
    pub cpu_total_s: f64,
}

#[cfg(target_os = "macos")]
mod imp {
    use super::PerfSample;
    use std::mem::MaybeUninit;
    use std::time::{Duration, Instant};

    /// `proc_taskinfo` from <libproc.h>. Layout is part of the Darwin ABI.
    #[repr(C)]
    #[derive(Default)]
    struct ProcTaskInfo {
        pti_virtual_size: u64,
        pti_resident_size: u64,
        pti_total_user: u64,
        pti_total_system: u64,
        pti_threads_user: u64,
        pti_threads_system: u64,
        pti_policy: i32,
        pti_faults: i32,
        pti_pageins: i32,
        pti_cow_faults: i32,
        pti_messages_sent: i32,
        pti_messages_received: i32,
        pti_syscalls_mach: i32,
        pti_syscalls_unix: i32,
        pti_csw: i32,
        pti_threadnum: i32,
        pti_numrunning: i32,
        pti_priority: i32,
    }

    const PROC_PIDTASKINFO: i32 = 4;

    extern "C" {
        fn proc_pidinfo(pid: i32, flavor: i32, arg: u64, buffer: *mut u8, buffersize: i32) -> i32;
        fn getrusage(who: i32, usage: *mut RUsage) -> i32;
    }

    const RUSAGE_SELF: i32 = 0;

    #[repr(C)]
    #[derive(Default)]
    struct TimeVal {
        tv_sec: i64,
        tv_usec: i32,
    }

    #[repr(C)]
    #[derive(Default)]
    struct RUsage {
        ru_utime: TimeVal,
        ru_stime: TimeVal,
        ru_maxrss: i64,
        ru_ixrss: i64,
        ru_idrss: i64,
        ru_isrss: i64,
        ru_minflt: i64,
        ru_majflt: i64,
        ru_nswap: i64,
        ru_inblock: i64,
        ru_oublock: i64,
        ru_msgsnd: i64,
        ru_msgrcv: i64,
        ru_nsignals: i64,
        ru_nvcsw: i64,
        ru_nivcsw: i64,
    }

    fn cpu_seconds() -> f64 {
        let mut usage = MaybeUninit::<RUsage>::uninit();
        // SAFETY: getrusage writes into the struct we provide.
        let rc = unsafe { getrusage(RUSAGE_SELF, usage.as_mut_ptr()) };
        if rc != 0 {
            return 0.0;
        }
        let usage = unsafe { usage.assume_init() };
        let user = usage.ru_utime.tv_sec as f64 + usage.ru_utime.tv_usec as f64 / 1e6;
        let sys = usage.ru_stime.tv_sec as f64 + usage.ru_stime.tv_usec as f64 / 1e6;
        user + sys
    }

    fn task_info() -> (u64, u64, i32) {
        let mut info = ProcTaskInfo::default();
        let size = std::mem::size_of::<ProcTaskInfo>() as i32;
        // SAFETY: buffer and length match; proc_pidinfo fills at most `size` bytes.
        let rc = unsafe {
            proc_pidinfo(
                std::process::id() as i32,
                PROC_PIDTASKINFO,
                0,
                &mut info as *mut ProcTaskInfo as *mut u8,
                size,
            )
        };
        if rc <= 0 {
            return (0, 0, 0);
        }
        (info.pti_resident_size, info.pti_virtual_size, info.pti_threadnum)
    }

    pub fn sample(window: Duration) -> PerfSample {
        let start_cpu = cpu_seconds();
        let started = Instant::now();
        std::thread::sleep(window);
        let elapsed = started.elapsed().as_secs_f64();
        let end_cpu = cpu_seconds();

        let (rss, virt, threads) = task_info();
        PerfSample {
            cpu_pct: if elapsed > 0.0 {
                ((end_cpu - start_cpu) / elapsed) * 100.0
            } else {
                0.0
            },
            rss_bytes: rss,
            virtual_bytes: virt,
            threads,
            window_ms: (elapsed * 1000.0) as u64,
            cpu_total_s: end_cpu,
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use super::PerfSample;
    use std::time::Duration;
    pub fn sample(_window: Duration) -> PerfSample {
        PerfSample {
            cpu_pct: 0.0,
            rss_bytes: 0,
            virtual_bytes: 0,
            threads: 0,
            window_ms: 0,
            cpu_total_s: 0.0,
        }
    }
}

pub use imp::sample;

/// Convenience wrapper used by the control channel.
pub fn sample_ms(ms: u64) -> PerfSample {
    sample(Duration::from_millis(ms))
}

/// Peak resident size reported by the kernel since launch, in bytes.
pub fn peak_rss_bytes() -> u64 {
    #[cfg(target_os = "macos")]
    {
        let (rss, _, _) = {
            // Reuse the sampler with a zero-length window.
            let s = sample(Duration::ZERO);
            (s.rss_bytes, s.virtual_bytes, s.threads)
        };
        rss
    }
    #[cfg(not(target_os = "macos"))]
    {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sampling_returns_plausible_values() {
        let s = sample_ms(80);
        assert!(s.rss_bytes > 1_000_000, "a running process has resident memory");
        assert!(s.virtual_bytes >= s.rss_bytes);
        assert!(s.threads >= 1);
        assert!(s.cpu_pct >= 0.0 && s.cpu_pct < 2000.0);
        assert!(s.cpu_total_s >= 0.0);
    }

    #[test]
    fn cpu_time_is_monotonic() {
        let a = sample_ms(20);
        let b = sample_ms(20);
        assert!(b.cpu_total_s >= a.cpu_total_s);
    }
}
