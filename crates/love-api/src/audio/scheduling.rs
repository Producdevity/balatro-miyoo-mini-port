pub(super) fn configure() {
    if std::env::var("BALATRO_AUDIO_PRIORITY").as_deref() != Ok("realtime") {
        return;
    }
    #[cfg(target_os = "linux")]
    {
        // Only the mixer gets real-time priority. Device writes pace the worker.
        let mut parameter: libc::sched_param = unsafe { std::mem::zeroed() };
        parameter.sched_priority = 1;
        let result = unsafe {
            libc::pthread_setschedparam(libc::pthread_self(), libc::SCHED_RR, &parameter)
        };
        if result == 0 {
            eprintln!("[audio] scheduling: SCHED_RR, priority 1");
        } else {
            eprintln!(
                "[audio] using normal scheduling: {}",
                std::io::Error::from_raw_os_error(result)
            );
        }
    }
}
