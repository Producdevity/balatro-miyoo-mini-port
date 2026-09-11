//! ARM diagnostic builds only. Samples one selected thread's CPU clock, not wall time.
use std::io;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};

const BUCKET_SHIFT: usize = 5;
const BUCKETS: usize = 262_144;
// Linux uapi asm-generic/siginfo.h; libc does not expose it on musl.
const SIGEV_THREAD_ID: libc::c_int = 4;
static BASE: AtomicUsize = AtomicUsize::new(0);
static COUNTS: [AtomicU32; BUCKETS] = [const { AtomicU32::new(0) }; BUCKETS];
static OUTSIDE: AtomicU32 = AtomicU32::new(0);
static OWNED: AtomicBool = AtomicBool::new(false);

struct Ownership;

impl Drop for Ownership {
    fn drop(&mut self) {
        OWNED.store(false, Ordering::Release);
    }
}

extern "C" fn sample(_: libc::c_int, _: *mut libc::siginfo_t, context: *mut libc::c_void) {
    // Linux supplies a valid ucontext for SA_SIGINFO. Only lock-free atomics
    // are touched here; symbol lookup and output happen after timer shutdown.
    let pc = unsafe { (*(context as *const libc::ucontext_t)).uc_mcontext.arm_pc as usize };
    let slot = pc.wrapping_sub(BASE.load(Ordering::Relaxed)) >> BUCKET_SHIFT;
    if let Some(count) = COUNTS.get(slot) {
        count.fetch_add(1, Ordering::Relaxed);
    } else {
        OUTSIDE.fetch_add(1, Ordering::Relaxed);
    }
}

pub struct Sampler {
    timer: libc::timer_t,
    previous_action: libc::sigaction,
    previous_mask: libc::sigset_t,
    _ownership: Ownership,
    _same_thread: std::marker::PhantomData<*mut ()>,
}

impl Sampler {
    pub fn for_thread(name: &str) -> io::Result<Option<Self>> {
        let selected =
            std::env::var("BALATRO_NATIVE_PROFILE_THREAD").unwrap_or_else(|_| "raster".to_owned());
        if selected != "raster" && selected != "main" {
            return Err(io::Error::other("profile thread must be raster or main"));
        }
        if selected != name {
            return Ok(None);
        }
        eprintln!("[native-sample] thread={name}");
        Self::start().map(Some)
    }

    fn start() -> io::Result<Self> {
        if OWNED
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "CPU sampler is already running",
            ));
        }
        let ownership = Ownership;
        let executable = std::fs::read_link("/proc/self/exe")?;
        let maps = std::fs::read_to_string("/proc/self/maps")?;
        let mapping = maps
            .lines()
            .find(|line| line.contains("r-xp") && line.ends_with(&*executable.to_string_lossy()))
            .ok_or_else(|| io::Error::other("executable mapping not found"))?;
        let range = mapping.split_whitespace().next().unwrap_or("");
        let (start, end) = range
            .split_once('-')
            .ok_or_else(|| io::Error::other("invalid executable mapping"))?;
        let parse =
            |value| usize::from_str_radix(value, 16).map_err(|error| io::Error::other(error));
        let base = parse(start)?;
        let end = parse(end)?;
        if end.saturating_sub(base) > BUCKETS << BUCKET_SHIFT {
            return Err(io::Error::other("executable exceeds sampler range"));
        }
        BASE.store(base, Ordering::Relaxed);
        for count in &COUNTS {
            count.store(0, Ordering::Relaxed);
        }
        OUTSIDE.store(0, Ordering::Relaxed);

        // The diagnostic build owns SIGPROF only for the selected thread's scope.
        unsafe {
            let mut previous_action: libc::sigaction = std::mem::zeroed();
            if libc::sigaction(libc::SIGPROF, std::ptr::null(), &mut previous_action) != 0 {
                return Err(io::Error::last_os_error());
            }
            if previous_action.sa_sigaction != libc::SIG_DFL {
                return Err(io::Error::other("SIGPROF already has a handler"));
            }
            let mut action: libc::sigaction = std::mem::zeroed();
            action.sa_sigaction = sample as *const () as usize;
            action.sa_flags = libc::SA_SIGINFO | libc::SA_RESTART;
            libc::sigemptyset(&mut action.sa_mask);
            if libc::sigaction(libc::SIGPROF, &action, std::ptr::null_mut()) != 0 {
                return Err(io::Error::last_os_error());
            }
            let mut signals: libc::sigset_t = std::mem::zeroed();
            libc::sigemptyset(&mut signals);
            libc::sigaddset(&mut signals, libc::SIGPROF);
            let mut previous_mask = std::mem::zeroed();
            let error = libc::pthread_sigmask(libc::SIG_UNBLOCK, &signals, &mut previous_mask);
            if error != 0 {
                libc::sigaction(libc::SIGPROF, &previous_action, std::ptr::null_mut());
                return Err(io::Error::from_raw_os_error(error));
            }
            let mut event: libc::sigevent = std::mem::zeroed();
            event.sigev_notify = SIGEV_THREAD_ID;
            event.sigev_signo = libc::SIGPROF;
            event.sigev_notify_thread_id = libc::syscall(libc::SYS_gettid) as libc::pid_t;
            let mut timer = std::mem::zeroed();
            if libc::timer_create(libc::CLOCK_THREAD_CPUTIME_ID, &mut event, &mut timer) != 0 {
                let error = io::Error::last_os_error();
                libc::sigaction(libc::SIGPROF, &previous_action, std::ptr::null_mut());
                libc::pthread_sigmask(libc::SIG_SETMASK, &previous_mask, std::ptr::null_mut());
                return Err(error);
            }
            let sampler = Self {
                timer,
                previous_action,
                previous_mask,
                _ownership: ownership,
                _same_thread: std::marker::PhantomData,
            };
            let tick = libc::timespec {
                tv_sec: 0,
                tv_nsec: 2_000_000,
            };
            let interval = libc::itimerspec {
                it_interval: tick,
                it_value: tick,
            };
            if libc::timer_settime(timer, 0, &interval, std::ptr::null_mut()) != 0 {
                return Err(io::Error::last_os_error());
            }
            eprintln!("[native-sample] interval_us=2000 bucket_bytes=32 mapping={mapping}");
            Ok(sampler)
        }
    }
}

impl Drop for Sampler {
    fn drop(&mut self) {
        unsafe {
            let mut signals: libc::sigset_t = std::mem::zeroed();
            libc::sigemptyset(&mut signals);
            libc::sigaddset(&mut signals, libc::SIGPROF);
            libc::pthread_sigmask(libc::SIG_BLOCK, &signals, std::ptr::null_mut());
            libc::timer_delete(self.timer);
            let zero = libc::timespec {
                tv_sec: 0,
                tv_nsec: 0,
            };
            while libc::sigtimedwait(&signals, std::ptr::null_mut(), &zero) >= 0 {}
            libc::sigaction(libc::SIGPROF, &self.previous_action, std::ptr::null_mut());
            libc::pthread_sigmask(libc::SIG_SETMASK, &self.previous_mask, std::ptr::null_mut());
        }
        let base = BASE.load(Ordering::Relaxed);
        let mut total = 0_u64;
        for (index, count) in COUNTS.iter().enumerate() {
            let count = count.load(Ordering::Relaxed);
            if count != 0 {
                total += u64::from(count);
                eprintln!(
                    "[native-sample] pc={:#x} count={count}",
                    base + (index << BUCKET_SHIFT)
                );
            }
        }
        eprintln!(
            "[native-sample] total={total} outside={}",
            OUTSIDE.load(Ordering::Relaxed)
        );
    }
}
