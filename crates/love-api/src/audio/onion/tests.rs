use super::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "balatro-audio-reconnect-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn command(&self) -> Command {
        let mut command = Command::new("sh");
        command.current_dir(&self.0).args([
            "-c",
            "if [ -e stalled ]; then trap '' TERM; exec sleep 30; fi; exec cat >> output.pcm",
        ]);
        command
    }

    fn wait_bytes(&self, count: u64) -> Vec<u8> {
        let deadline = Instant::now() + Duration::from_secs(2);
        let path = self.0.join("output.pcm");
        while std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0) < count {
            assert!(Instant::now() < deadline, "helper did not receive PCM");
            std::thread::sleep(Duration::from_millis(5));
        }
        std::fs::read(path).unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn reconnect_keeps_pcm_frames_and_discards_disconnected_audio() {
    let fixture = Fixture::new();
    let mut output = OnionOutput::start(fixture.command()).unwrap();
    output.write_samples(&[100, -100]).unwrap();
    fixture.wait_bytes(4);
    for cycle in 0..3 {
        output.connection.as_mut().unwrap().child.kill().unwrap();
        output.connection.as_mut().unwrap().child.wait().unwrap();
        output.write_samples(&[200, -200]).unwrap();
        assert!(output.connection.is_none());
        let retry_at = output.retry_at;
        let started = Instant::now();
        for _ in 0..4 {
            output.write_samples(&[300; 512]).unwrap();
        }
        assert!(started.elapsed() >= Duration::from_millis(20));
        assert_eq!(retry_at, output.retry_at, "retried during backoff");
        output.retry_at = Instant::now();
        output.write_samples(&[100, -100]).unwrap();
        assert!(output.connection.is_some());
        fixture.wait_bytes((cycle + 2) * 4);
    }
    assert_eq!(fixture.wait_bytes(16), [100, 0, 156, 255].repeat(4));
}

#[test]
fn stalled_helper_is_bounded_and_can_be_replaced() {
    let fixture = Fixture::new();
    std::fs::write(fixture.0.join("stalled"), "").unwrap();
    let mut output = OnionOutput::start(fixture.command()).unwrap();
    let pid = output.connection.as_ref().unwrap().child.id() as libc::pid_t;
    let started = Instant::now();
    output.write_samples(&vec![0; 128 * 1024]).unwrap();
    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(output.connection.is_none());
    assert_eq!(unsafe { libc::kill(pid, 0) }, -1, "helper was not reaped");
    std::fs::remove_file(fixture.0.join("stalled")).unwrap();
    output.retry_at = Instant::now();
    output.write_samples(&[32767, -32768]).unwrap();
    assert_eq!(fixture.wait_bytes(4), [255, 127, 0, 128]);
}

#[test]
fn repeated_spawn_failure_uses_capped_backoff() {
    let fixture = Fixture::new();
    let mut output = OnionOutput::start(fixture.command()).unwrap();
    output.connection.take();
    output.command = Command::new(fixture.0.join("missing-helper"));
    for _ in 0..5 {
        output.retry_at = Instant::now();
        output.write_samples(&[0; 512]).unwrap();
        assert!(output.connection.is_none());
    }
    assert_eq!(output.retry_delay, Duration::from_secs(5));
}

#[test]
fn shutdown_does_not_wait_forever_for_a_stalled_helper() {
    let fixture = Fixture::new();
    std::fs::write(fixture.0.join("stalled"), "").unwrap();
    let output = OnionOutput::start(fixture.command()).unwrap();
    std::thread::sleep(Duration::from_millis(50));
    let started = Instant::now();
    drop(output);
    assert!(started.elapsed() < Duration::from_secs(2));
}
