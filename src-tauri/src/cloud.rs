//! Cloud storage through rclone (https://rclone.org), which talks to Google Drive,
//! Backblaze B2, Proton Drive and dozens more. Remotes are set up once with
//! `rclone config`; the app only runs rclone commands on them, so rclone must be installed.
//!
//! Paths on a remote are rclone's `remote:folder/file` strings.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};
use crate::library::Library;

const SETTING: &str = "rclone_path";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Remote {
    pub name: String,
    /// rclone backend, e.g. `drive`, `b2`, `protondrive`.
    pub kind: String,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RcloneStatus {
    /// The rclone program used, if one was found.
    pub path: Option<String>,
    pub version: Option<String>,
    pub remotes: Vec<Remote>,
    /// Why rclone can't be used, or why its remotes couldn't be listed.
    pub error: Option<String>,
}

/// What a copy did. A failed run still reports the files copied before it failed.
#[derive(Debug, Default)]
pub struct CopyOutcome {
    /// Per-file and overall failures.
    pub errors: Vec<String>,
    pub cancelled: bool,
}

pub struct Rclone {
    bin: PathBuf,
}

/// Places rclone's installers put it that a desktop app's PATH may miss (apps started from
/// the macOS Dock or Finder don't get the shell's PATH).
fn extra_dirs() -> Vec<PathBuf> {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from);
    let mut dirs: Vec<PathBuf> = vec!["/opt/homebrew/bin".into(), "/usr/local/bin".into(), "/usr/bin".into()];
    if let Some(home) = &home {
        dirs.extend([home.join(".local/bin"), home.join("bin"), home.join("scoop/shims")]);
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) {
        dirs.push(local.join("Microsoft/WinGet/Links"));
    }
    dirs
}

fn exe_name() -> &'static str {
    if cfg!(windows) { "rclone.exe" } else { "rclone" }
}

/// The rclone program chosen in the Backup view, if any.
pub fn chosen_path(lib: &Library) -> Result<Option<String>> {
    Ok(lib
        .conn()
        .query_row("SELECT value FROM settings WHERE key = ?1", [SETTING], |r| r.get(0))
        .optional()?)
}

/// Uses this rclone program from now on; `None` goes back to finding it on PATH.
pub fn set_path(lib: &Library, path: Option<&str>) -> Result<()> {
    match path.map(str::trim).filter(|p| !p.is_empty()) {
        Some(p) => {
            Rclone { bin: p.into() }.version()?;
            lib.conn().execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT (key) DO UPDATE SET value = ?2",
                [SETTING, p],
            )?;
        }
        None => {
            lib.conn().execute("DELETE FROM settings WHERE key = ?1", [SETTING])?;
        }
    }
    Ok(())
}

/// rclone's state for the Backup view: where it is and which remotes are set up.
pub fn status(lib: &Library) -> RcloneStatus {
    let rc = match Rclone::find(lib) {
        Ok(rc) => rc,
        Err(e) => return RcloneStatus { error: Some(e.to_string()), ..Default::default() },
    };
    let mut status = RcloneStatus { path: Some(rc.bin.to_string_lossy().into_owned()), ..Default::default() };
    match rc.version().and_then(|v| Ok((v, rc.remotes()?))) {
        Ok((version, remotes)) => {
            status.version = Some(version);
            status.remotes = remotes;
        }
        Err(e) => status.error = Some(e.to_string()),
    }
    status
}

/// The remote's name in a `remote:path` string.
pub fn remote_name(path: &str) -> Option<&str> {
    path.split_once(':').map(|(name, _)| name).filter(|n| !n.is_empty())
}

/// `base` + `/` + `name`, without doubling the separator after `remote:` or `/`.
pub fn join(base: &str, name: &str) -> String {
    if base.ends_with(':') || base.ends_with('/') {
        format!("{base}{name}")
    } else {
        format!("{base}/{name}")
    }
}

/// One line of rclone's `--use-json-log` output.
#[derive(Deserialize)]
struct LogLine {
    level: String,
    msg: String,
    object: Option<String>,
}

#[derive(Default)]
struct Finished {
    success: bool,
    /// rclone's exit code; 3 and 4 mean the directory or file doesn't exist.
    code: Option<i32>,
    cancelled: bool,
    /// Error-level log lines, as `object: message`.
    errors: Vec<String>,
    /// The last notice, which holds rclone's summary of a failure ("Failed to copy: …").
    last_notice: Option<String>,
}

impl Finished {
    fn not_found(&self) -> bool {
        matches!(self.code, Some(3 | 4))
    }

    fn message(&self) -> String {
        self.errors
            .first()
            .cloned()
            .or_else(|| self.last_notice.clone())
            .unwrap_or_else(|| format!("rclone failed (exit code {:?})", self.code))
    }

    fn into_result(self) -> Result<Self> {
        if self.success {
            Ok(self)
        } else {
            Err(AppError::msg(format!("rclone: {}", self.message())))
        }
    }
}

enum Out {
    Stdout(String),
    Stderr(String),
}

impl Rclone {
    /// The rclone program chosen in the Backup view, else the one on PATH.
    pub fn find(lib: &Library) -> Result<Rclone> {
        if let Some(path) = chosen_path(lib)? {
            return if Path::new(&path).is_file() {
                Ok(Rclone { bin: path.into() })
            } else {
                Err(AppError::msg(format!("rclone is no longer at {path}")))
            };
        }
        let path_dirs = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_else(Vec::new);
        path_dirs
            .into_iter()
            .chain(extra_dirs())
            .map(|d| d.join(exe_name()))
            .find(|p| p.is_file())
            .map(|bin| Rclone { bin })
            .ok_or_else(|| AppError::msg("rclone is not installed"))
    }

    fn command(&self) -> Command {
        let mut cmd = Command::new(&self.bin);
        // Never wait for a config password on a terminal nobody sees.
        cmd.args(["--use-json-log", "--ask-password=false"]);
        cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        cmd
    }

    /// Runs rclone to the end, feeding `stdin` and passing each stdout line and log line
    /// to the callbacks. Kills rclone when `cancel` is set.
    fn run(
        &self,
        mut cmd: Command,
        stdin: Option<String>,
        cancel: Option<&AtomicBool>,
        mut on_stdout: impl FnMut(&str),
        mut on_log: impl FnMut(&LogLine),
    ) -> Result<Finished> {
        if stdin.is_some() {
            cmd.stdin(Stdio::piped());
        }
        let mut child = cmd.spawn().map_err(|e| AppError::msg(format!("can't run rclone ({}): {e}", self.bin.display())))?;
        if let (Some(input), Some(mut pipe)) = (stdin, child.stdin.take()) {
            std::thread::spawn(move || {
                let _ = pipe.write_all(input.as_bytes());
            });
        }
        let (tx, rx) = mpsc::channel();
        let readers = [
            child.stdout.take().map(|p| (Box::new(p) as Box<dyn std::io::Read + Send>, true)),
            child.stderr.take().map(|p| (Box::new(p) as Box<dyn std::io::Read + Send>, false)),
        ];
        for (pipe, is_stdout) in readers.into_iter().flatten() {
            let tx = tx.clone();
            std::thread::spawn(move || {
                for line in BufReader::new(pipe).lines().map_while(|l| l.ok()) {
                    let _ = tx.send(if is_stdout { Out::Stdout(line) } else { Out::Stderr(line) });
                }
            });
        }
        drop(tx);

        let mut done = Finished::default();
        loop {
            if cancel.is_some_and(|c| c.load(Ordering::SeqCst)) && !done.cancelled {
                done.cancelled = true;
                let _ = child.kill();
            }
            match rx.recv_timeout(Duration::from_millis(200)) {
                Ok(Out::Stdout(line)) => on_stdout(&line),
                Ok(Out::Stderr(line)) => match serde_json::from_str::<LogLine>(&line) {
                    Ok(log) => {
                        match log.level.as_str() {
                            "error" | "critical" | "alert" | "emergency" => done.errors.push(match &log.object {
                                Some(obj) => format!("{obj}: {}", log.msg),
                                None => log.msg.clone(),
                            }),
                            "notice" => done.last_notice = Some(log.msg.clone()),
                            _ => {}
                        }
                        on_log(&log);
                    }
                    // Not a log line, e.g. a crash; keep it for the error message.
                    Err(_) if !line.trim().is_empty() => done.last_notice = Some(line),
                    Err(_) => {}
                },
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
        let status = child.wait()?;
        done.success = status.success() && !done.cancelled;
        done.code = status.code();
        Ok(done)
    }

    /// Runs a quick command and returns its stdout.
    fn output(&self, args: &[&str], stdin: Option<String>) -> Result<(String, Finished)> {
        let mut cmd = self.command();
        cmd.args(args);
        let mut out = String::new();
        let done = self.run(cmd, stdin, None, |l| {
            out.push_str(l);
            out.push('\n');
        }, |_| {})?;
        Ok((out, done))
    }

    pub fn version(&self) -> Result<String> {
        let (out, done) = self.output(&["version"], None)?;
        done.into_result()?;
        out.lines()
            .next()
            .and_then(|l| l.strip_prefix("rclone "))
            .map(|v| v.trim().to_string())
            .ok_or_else(|| AppError::msg(format!("{} is not rclone", self.bin.display())))
    }

    pub fn remotes(&self) -> Result<Vec<Remote>> {
        let (out, done) = self.output(&["listremotes", "--long"], None)?;
        done.into_result()?;
        Ok(out
            .lines()
            .filter_map(|l| l.split_once(':'))
            .map(|(name, kind)| Remote { name: name.trim().to_string(), kind: kind.trim().to_string() })
            .filter(|r| !r.name.is_empty())
            .collect())
    }

    /// A file's contents, or `None` when it doesn't exist.
    pub fn read(&self, path: &str) -> Result<Option<Vec<u8>>> {
        let (out, done) = self.output(&["cat", path], None)?;
        if done.not_found() {
            return Ok(None);
        }
        done.into_result()?;
        Ok(Some(out.into_bytes()))
    }

    pub fn write(&self, path: &str, contents: &str) -> Result<()> {
        self.output(&["rcat", path], Some(contents.to_string()))?.1.into_result()?;
        Ok(())
    }

    pub fn upload(&self, src: &Path, dest: &str) -> Result<()> {
        self.output(&["copyto", &src.to_string_lossy(), dest], None)?.1.into_result()?;
        Ok(())
    }

    pub fn delete(&self, path: &str) -> Result<()> {
        self.output(&["deletefile", path], None)?.1.into_result()?;
        Ok(())
    }

    /// Sizes of the files under `root` by path relative to it, limited to `only` when given.
    /// A missing `root` has no files.
    pub fn sizes(&self, root: &str, only: Option<&[String]>) -> Result<HashMap<String, i64>> {
        #[derive(Deserialize)]
        struct Entry {
            #[serde(rename = "Path")]
            path: String,
            #[serde(rename = "Size")]
            size: i64,
        }
        let mut args = vec!["lsjson", "-R", "--files-only", "--no-mimetype", "--no-modtime", root];
        if only.is_some() {
            args.extend(["--files-from-raw", "-"]);
        }
        let (out, done) = self.output(&args, only.map(|files| files.join("\n")))?;
        if done.not_found() {
            return Ok(HashMap::new());
        }
        done.into_result()?;
        let entries: Vec<Entry> =
            serde_json::from_str(&out).map_err(|e| AppError::msg(format!("rclone lsjson: {e}")))?;
        Ok(entries.into_iter().map(|e| (e.path, e.size)).collect())
    }

    /// Copies `files` (relative to `src`) to the same paths under `dest`. rclone checks each
    /// upload's hash itself. A different file already at a path is kept, renamed with
    /// `suffix`. `on_copied` gets each path once it's uploaded.
    pub fn copy(
        &self,
        src: &Path,
        dest: &str,
        files: &[String],
        suffix: &str,
        cancel: &AtomicBool,
        mut on_copied: impl FnMut(&str),
    ) -> Result<CopyOutcome> {
        let mut cmd = self.command();
        cmd.args(["copy", "-v", "--stats", "0", "--files-from-raw", "-", "--suffix-keep-extension"]);
        cmd.arg(format!("--suffix={suffix}")).arg(src).arg(dest);
        let done = self.run(cmd, Some(files.join("\n")), Some(cancel), |_| {}, |log| {
            if let (true, Some(obj)) = (log.msg.starts_with("Copied"), &log.object) {
                on_copied(obj);
            }
        })?;
        let mut outcome = CopyOutcome { cancelled: done.cancelled, ..Default::default() };
        if !done.success && !done.cancelled && done.errors.is_empty() {
            outcome.errors.push(done.message());
        }
        outcome.errors.extend(done.errors);
        Ok(outcome)
    }

    /// Compares `files` under `src` and `dest` by size and hash, passing each file's result
    /// to `on_result`: `=` same, `*` different, `+` missing on `dest`, `!` unreadable.
    /// Returns whether it was cancelled.
    pub fn check(
        &self,
        src: &Path,
        dest: &str,
        files: &[String],
        cancel: &AtomicBool,
        mut on_result: impl FnMut(char, &str),
    ) -> Result<bool> {
        let mut cmd = self.command();
        cmd.args(["check", "--one-way", "--combined", "-", "--files-from-raw", "-"]).arg(src).arg(dest);
        let mut results = 0;
        let done = self.run(cmd, Some(files.join("\n")), Some(cancel), |line| {
            let mut chars = line.chars();
            if let (Some(sym), Some(' ')) = (chars.next(), chars.next()) {
                results += 1;
                on_result(sym, chars.as_str());
            }
        }, |_| {})?;
        // Differences make rclone fail too; only a run with no results is an error.
        if !done.success && !done.cancelled && results == 0 {
            return Err(AppError::msg(format!("rclone: {}", done.message())));
        }
        Ok(done.cancelled)
    }
}
