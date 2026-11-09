//! Bounded startup and installation output, with private persistent diagnostic logs.
use crate::{prompt, Result};
use console::{style, Term};
use std::{
    collections::VecDeque,
    fs::File,
    io::{self, IsTerminal, Read, Seek, Write},
    os::unix::process::CommandExt,
    path::PathBuf,
    process::{Command, Output, Stdio},
    time::Duration,
};

struct Pane {
    term: Term,
    original: libc::termios,
    drawn: usize,
}

impl Pane {
    fn new() -> io::Result<Self> {
        // SAFETY: stdin is a checked terminal; all termios pointers are valid.
        unsafe {
            let mut original = std::mem::zeroed();
            if libc::tcgetattr(0, &mut original) != 0 {
                return Err(io::Error::last_os_error());
            }
            let mut mode = original;
            mode.c_lflag &= !(libc::ICANON | libc::ECHO | libc::ISIG);
            mode.c_cc[libc::VMIN] = 0;
            mode.c_cc[libc::VTIME] = 0;
            if libc::tcsetattr(0, libc::TCSANOW, &mode) != 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(Self {
                term: Term::stderr(),
                original,
                drawn: 0,
            })
        }
    }

    fn draw(&mut self, lines: &VecDeque<String>, offset: usize, label: &str) -> io::Result<()> {
        self.term.clear_last_lines(self.drawn)?;
        let (height, width) = self.term.size();
        let count = usize::from(height).saturating_sub(5).clamp(1, 8);
        let width = usize::from(width).saturating_sub(1).max(1);
        let end = lines.len().saturating_sub(offset);
        let heading = format!("{label} logs · ↑/k older · ↓/j newer · f follow · Ctrl-C cancel");
        self.term.write_line("")?;
        self.term.write_line(&format!(
            "  {}",
            style(console::truncate_str(&heading, width.saturating_sub(2), ""))
                .cyan()
                .dim()
        ))?;
        for line in lines.iter().take(end).skip(end.saturating_sub(count)) {
            self.term.write_line(&format!(
                "  {}",
                style(console::truncate_str(line, width.saturating_sub(2), ""))
                    .color256(245)
                    .dim()
            ))?;
        }
        self.drawn = 2 + end.min(count);
        Ok(())
    }
}

impl Drop for Pane {
    fn drop(&mut self) {
        let _ = self.term.clear_last_lines(self.drawn);
        // SAFETY: original was obtained from this terminal and remains valid.
        unsafe {
            libc::tcsetattr(0, libc::TCSANOW, &self.original);
        }
    }
}

pub fn prepare_log(install: bool) -> Result<tempfile::NamedTempFile> {
    let directory = std::env::temp_dir()
        .canonicalize()
        .map_err(|e| format!("resolve log directory: {e}"))?;
    tempfile::Builder::new()
        .prefix(if install {
            "hatch-install-"
        } else {
            "hatch-devcontainer-"
        })
        .suffix(".log")
        .tempfile_in(directory)
        .map_err(|e| format!("create log: {e}"))
}

pub fn run(
    command: &mut Command,
    interactive: bool,
    log: tempfile::NamedTempFile,
) -> Result<(Output, PathBuf)> {
    run_logged(command, interactive, false, log)
}

fn run_logged(
    command: &mut Command,
    interactive: bool,
    install: bool,
    log: tempfile::NamedTempFile,
) -> Result<(Output, PathBuf)> {
    let label = if install {
        "Installation"
    } else {
        "Dev container"
    };
    let operation = if install {
        "agent installer"
    } else {
        "Dev Container CLI"
    };
    // Keep only confirmed operations' logs. Cancellation before execution drops the file.
    let (mut log, path) = log.keep().map_err(|e| format!("retain {label} log: {e}"))?;
    let result = (|| -> io::Result<Output> {
        let mut stdout = tempfile::tempfile()?;
        let mut reader = File::open(&path)?;
        let mut pane = if interactive
            && !prompt::plain()
            && io::stdin().is_terminal()
            && io::stderr().is_terminal()
        {
            Some(Pane::new()?)
        } else {
            None
        };
        // Files avoid pipe backpressure. Keep the agent's stdin untouched.
        if pane.is_some() {
            command.process_group(0);
        }
        let isolated = pane.is_some();
        let mut child = command
            .stdin(Stdio::null())
            .stdout(if install {
                log.try_clone()?
            } else {
                stdout.try_clone()?
            })
            .stderr(log.try_clone()?)
            .spawn()
            .map_err(|e| io::Error::new(e.kind(), format!("could not start {operation}: {e}")))?;
        let result = (|| -> io::Result<Output> {
            let mut lines = VecDeque::from([String::new()]);
            let mut offset = 0usize;
            let mut keys = VecDeque::new();
            let mut line_start = true;
            loop {
                let status = child.try_wait()?;
                let mut buffer = [0u8; 16384];
                let n = reader.read(&mut buffer)?;
                if pane.is_some() {
                    for part in String::from_utf8_lossy(&buffer[..n]).split_inclusive(['\n', '\r'])
                    {
                        let ends = part.ends_with(['\n', '\r']);
                        let line = lines.back_mut().unwrap();
                        // Bound both individual lines and retained scrollback.
                        if line.len() < 4096 {
                            line.extend(
                                prompt::clean(part.trim_end_matches(['\n', '\r']))
                                    .chars()
                                    .take(4096 - line.len()),
                            );
                        }
                        if ends {
                            lines.push_back(String::new());
                            if offset > 0 {
                                offset += 1;
                            }
                            if lines.len() > 2000 {
                                lines.pop_front();
                            }
                        }
                    }
                } else {
                    let mut stderr = io::stderr().lock();
                    for part in buffer[..n].split_inclusive(|b| *b == b'\n') {
                        if line_start {
                            stderr.write_all(b"  ")?;
                        }
                        stderr.write_all(part)?;
                        line_start = part.ends_with(b"\n");
                    }
                }
                if let Some(pane) = &mut pane {
                    let mut input = [0u8; 64];
                    // SAFETY: input is writable; noncanonical terminal reads have VMIN=0.
                    let count = unsafe { libc::read(0, input.as_mut_ptr().cast(), input.len()) };
                    for &key in &input[..count.max(0) as usize] {
                        keys.push_back(key);
                        if keys.len() > 3 {
                            keys.pop_front();
                        }
                        match key {
                            3 => {
                                return Err(io::Error::new(
                                    io::ErrorKind::Interrupted,
                                    format!("{} cancelled", label.to_lowercase()),
                                ))
                            }
                            b'k' => offset = offset.saturating_add(4),
                            b'j' => offset = offset.saturating_sub(4),
                            b'f' => offset = 0,
                            b'A' if keys.iter().copied().eq([27, b'[', b'A']) => {
                                offset = offset.saturating_add(4)
                            }
                            b'B' if keys.iter().copied().eq([27, b'[', b'B']) => {
                                offset = offset.saturating_sub(4)
                            }
                            _ => (),
                        }
                    }
                    offset = offset.min(lines.len().saturating_sub(1));
                    if n > 0 || count > 0 || pane.drawn == 0 {
                        pane.draw(&lines, offset, label)?;
                    }
                }
                if let Some(status) = status {
                    if n > 0 {
                        continue;
                    }
                    stdout.rewind()?;
                    let mut bytes = Vec::new();
                    stdout.read_to_end(&mut bytes)?;
                    if !install {
                        log.write_all(b"\n--- Dev Container CLI result ---\n")?;
                        log.write_all(&bytes)?;
                    }
                    return Ok(Output {
                        status,
                        stdout: bytes,
                        stderr: Vec::new(),
                    });
                }
                if n < buffer.len() {
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
        })();
        if result.is_err() {
            // SAFETY: the child owns this process group. Stop hooks as well as the CLI.
            unsafe {
                libc::kill(
                    if isolated {
                        -(child.id() as i32)
                    } else {
                        child.id() as i32
                    },
                    libc::SIGKILL,
                );
            }
            let _ = child.wait();
        }
        result
    })();
    result
        .map(|output| (output, path.clone()))
        .map_err(|e| format!("{operation}: {e}. {label} log: {}", path.display()))
}
