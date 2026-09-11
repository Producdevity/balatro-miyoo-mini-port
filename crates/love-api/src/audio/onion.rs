use anyhow::{Context, Result};
use std::io::{self, Write};
use std::os::fd::AsRawFd;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::time::{Duration, Instant};

const WRITE_TIMEOUT: Duration = Duration::from_millis(200);
const RETRY_DELAY: Duration = Duration::from_millis(500);

pub(super) struct OnionOutput {
    command: Command,
    connection: Option<Connection>,
    retry_at: Instant,
    retry_delay: Duration,
}

impl OnionOutput {
    pub(super) fn open() -> Result<Self> {
        let helper = std::env::current_exe()?.with_file_name("balatro-audio");
        let interposer = "/mnt/SDCARD/miyoo/lib/libpadsp.so";
        anyhow::ensure!(helper.is_file(), "Onion audio helper is missing");
        anyhow::ensure!(
            std::path::Path::new(interposer).is_file(),
            "Onion audio library is missing"
        );
        let mut command = Command::new(&helper);
        command
            .env("LD_PRELOAD", interposer)
            .env_remove("PADSP_NO_DSP");
        Self::start(command).with_context(|| format!("start {}", helper.display()))
    }

    fn start(mut command: Command) -> Result<Self> {
        let connection = Connection::open(&mut command)?;
        Ok(Self {
            command,
            connection: Some(connection),
            retry_at: Instant::now(),
            retry_delay: RETRY_DELAY,
        })
    }

    pub(super) fn write_samples(&mut self, samples: &[i16]) -> Result<()> {
        let started = Instant::now();
        if self.connection.is_none() && started >= self.retry_at {
            match Connection::open(&mut self.command) {
                Ok(connection) => self.connection = Some(connection),
                Err(error) => self.disconnected(error),
            }
        }
        if let Some(connection) = &mut self.connection {
            let bytes = unsafe {
                std::slice::from_raw_parts(
                    samples.as_ptr().cast::<u8>(),
                    std::mem::size_of_val(samples),
                )
            };
            match connection.write(bytes) {
                Ok(()) => {
                    if connection.bytes_written >= super::OUTPUT_RATE as usize * 4 {
                        if self.retry_delay > RETRY_DELAY {
                            eprintln!("[audio] Onion output recovered");
                        }
                        self.retry_delay = RETRY_DELAY;
                    }
                    return Ok(());
                }
                Err(error) => self.disconnected(error.into()),
            }
        }
        // Keep playback and commands moving while the server is unavailable.
        // Missing audio is discarded, never queued for a burst after wake-up.
        let period =
            Duration::from_secs_f64(samples.len() as f64 / (2.0 * super::OUTPUT_RATE as f64));
        std::thread::sleep(period.saturating_sub(started.elapsed()));
        Ok(())
    }

    fn disconnected(&mut self, error: anyhow::Error) {
        self.connection.take();
        eprintln!("[audio] Onion output disconnected: {error:#}; retrying");
        self.retry_at = Instant::now() + self.retry_delay;
        self.retry_delay = (self.retry_delay * 2).min(Duration::from_secs(5));
    }
}

struct Connection {
    child: Child,
    input: Option<ChildStdin>,
    bytes_written: usize,
}

impl Connection {
    fn open(command: &mut Command) -> Result<Self> {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .spawn()?;
        let input = child.stdin.take();
        let connection = Self {
            child,
            input,
            bytes_written: 0,
        };
        let fd = connection.input.as_ref().unwrap().as_raw_fd();
        #[cfg(target_os = "linux")]
        {
            let size = unsafe { libc::fcntl(fd, libc::F_SETPIPE_SZ, 4096) };
            anyhow::ensure!(
                size > 0 && size <= 4096,
                "could not limit the mixer output queue"
            );
        }
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
            return Err(io::Error::last_os_error().into());
        }
        Ok(connection)
    }

    fn write(&mut self, mut bytes: &[u8]) -> io::Result<()> {
        if let Some(status) = self.child.try_wait()? {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                format!("helper exited: {status}"),
            ));
        }
        let deadline = Instant::now() + WRITE_TIMEOUT;
        let input = self.input.as_mut().unwrap();
        while !bytes.is_empty() {
            if Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "helper stopped reading",
                ));
            }
            match input.write(bytes) {
                Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
                Ok(count) => {
                    self.bytes_written = self.bytes_written.saturating_add(count);
                    bytes = &bytes[count..];
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    let mut poll = libc::pollfd {
                        fd: input.as_raw_fd(),
                        events: libc::POLLOUT,
                        revents: 0,
                    };
                    let timeout = deadline
                        .saturating_duration_since(Instant::now())
                        .as_millis() as i32;
                    let ready = unsafe { libc::poll(&mut poll, 1, timeout.max(1)) };
                    if ready < 0 && io::Error::last_os_error().kind() != io::ErrorKind::Interrupted
                    {
                        return Err(io::Error::last_os_error());
                    }
                    if poll.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
                        return Err(io::ErrorKind::BrokenPipe.into());
                    }
                }
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.input.take();
        if matches!(self.child.try_wait(), Ok(Some(_))) {
            return;
        }
        unsafe {
            libc::kill(self.child.id() as libc::pid_t, libc::SIGTERM);
        }
        let deadline = Instant::now() + Duration::from_millis(100);
        while Instant::now() < deadline {
            if matches!(self.child.try_wait(), Ok(Some(_))) {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests;
