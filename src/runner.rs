use crate::model::{ExecutionRequest, RawExecution};
use crate::platform;
use anyhow::{Context, Result};
use std::io::{self, Read, Write};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Once;
use std::thread;
use std::time::{Duration, Instant};
use wait_timeout::ChildExt;

static INTERRUPTED: AtomicBool = AtomicBool::new(false);
static INSTALL_HANDLER: Once = Once::new();

fn install_interrupt_handler() {
    INSTALL_HANDLER.call_once(|| {
        let _ = ctrlc::set_handler(|| {
            INTERRUPTED.store(true, Ordering::SeqCst);
        });
    });
}

#[derive(Debug)]
struct BoundedLog {
    head: Vec<u8>,
    tail: Vec<u8>,
    total: usize,
    head_limit: usize,
    tail_limit: usize,
}

impl BoundedLog {
    fn new(limit: usize) -> Self {
        let head_limit = limit / 2;
        Self {
            head: Vec::with_capacity(head_limit),
            tail: Vec::with_capacity(limit - head_limit),
            total: 0,
            head_limit,
            tail_limit: limit - head_limit,
        }
    }

    fn push(&mut self, bytes: &[u8]) {
        self.total += bytes.len();
        let remaining_head = self.head_limit.saturating_sub(self.head.len());
        let head_take = remaining_head.min(bytes.len());
        self.head.extend_from_slice(&bytes[..head_take]);
        let tail_bytes = &bytes[head_take..];
        if !tail_bytes.is_empty() {
            self.tail.extend_from_slice(tail_bytes);
            if self.tail.len() > self.tail_limit {
                let excess = self.tail.len() - self.tail_limit;
                self.tail.drain(..excess);
            }
        }
    }

    fn finish(self) -> (Vec<u8>, bool) {
        let truncated = self.total > self.head.len() + self.tail.len();
        if !truncated {
            let mut output = self.head;
            output.extend(self.tail);
            return (output, false);
        }
        let mut output = self.head;
        output.extend_from_slice(b"\n... <ASSUMEZERO_OUTPUT_TRUNCATED> ...\n");
        output.extend(self.tail);
        (output, true)
    }
}

fn capture<R: Read + Send + 'static>(
    mut reader: R,
    limit: usize,
    verbose: bool,
    stderr: bool,
) -> thread::JoinHandle<(Vec<u8>, bool)> {
    thread::spawn(move || {
        let mut bounded = BoundedLog::new(limit);
        let mut buffer = [0_u8; 8_192];
        let mut streamed = 0_usize;
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => {
                    bounded.push(&buffer[..count]);
                    if verbose && streamed < limit {
                        let allowed = (limit - streamed).min(count);
                        if stderr {
                            let _ = io::stderr().write_all(&buffer[..allowed]);
                        } else {
                            let _ = io::stdout().write_all(&buffer[..allowed]);
                        }
                        streamed += allowed;
                    }
                }
                Err(_) => break,
            }
        }
        bounded.finish()
    })
}

pub fn execute(request: &ExecutionRequest) -> Result<RawExecution> {
    install_interrupt_handler();
    INTERRUPTED.store(false, Ordering::SeqCst);

    let mut command = platform::command_for_program(&request.executable);
    command
        .args(&request.args)
        .current_dir(&request.cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if request.clear_env {
        command.env_clear();
    }
    command.envs(&request.env);

    let start = Instant::now();
    let mut child = command.spawn().with_context(|| {
        format!(
            "command `{}` could not start in the isolated workspace",
            request.executable.display()
        )
    })?;
    let stdout = child
        .stdout
        .take()
        .context("stdout capture was unavailable")?;
    let stderr = child
        .stderr
        .take()
        .context("stderr capture was unavailable")?;
    let stdout_thread = capture(stdout, request.log_limit_bytes, request.verbose, false);
    let stderr_thread = capture(stderr, request.log_limit_bytes, request.verbose, true);

    let timeout = Duration::from_secs(request.timeout_seconds);
    let mut timed_out = false;
    let mut interrupted = false;
    let status = loop {
        if INTERRUPTED.load(Ordering::SeqCst) {
            interrupted = true;
            let _ = child.kill();
            break child.wait().ok();
        }
        if start.elapsed() >= timeout {
            timed_out = true;
            let _ = child.kill();
            break child.wait().ok();
        }
        if let Some(status) = child.wait_timeout(Duration::from_millis(50))? {
            break Some(status);
        }
    };

    let (stdout, stdout_truncated) = stdout_thread
        .join()
        .unwrap_or_else(|_| (b"<stdout capture thread failed>".to_vec(), false));
    let (stderr, stderr_truncated) = stderr_thread
        .join()
        .unwrap_or_else(|_| (b"<stderr capture thread failed>".to_vec(), false));

    Ok(RawExecution {
        exit_code: status.and_then(|value| value.code()),
        duration_ms: start.elapsed().as_millis(),
        timed_out,
        interrupted,
        stdout,
        stderr,
        output_truncated: stdout_truncated || stderr_truncated,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_log_retains_start_and_end() {
        let mut log = BoundedLog::new(10);
        log.push(b"abcdefghijklmnopqrst");
        let (output, truncated) = log.finish();
        let text = String::from_utf8(output).expect("UTF-8");
        assert!(truncated);
        assert!(text.starts_with("abcde"));
        assert!(text.ends_with("pqrst"));
        assert!(text.contains("TRUNCATED"));
    }
}
