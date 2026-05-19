//! Spawn `codex exec` as a subprocess and capture the assistant's last
//! message. Mirrors the Node.js `runCodexHeadless` pattern documented in
//! `docs/CODEX_CLI_INTEGRATION.md`.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

#[cfg(unix)]
use std::os::unix::process::CommandExt;

const DEFAULT_TIMEOUT_MS: u64 = 60_000;

#[derive(Debug, Clone)]
pub struct CodexResult {
    pub content: String,
    pub model: String,
    pub elapsed_ms: u128,
}

#[derive(Debug, Clone)]
pub struct CodexOptions {
    pub model: Option<String>,
    pub cwd: Option<PathBuf>,
    pub timeout_ms: Option<u64>,
    pub sandbox: Option<String>,
}

impl Default for CodexOptions {
    fn default() -> Self {
        Self {
            model: None,
            cwd: None,
            timeout_ms: None,
            sandbox: None,
        }
    }
}

fn codex_binary() -> String {
    std::env::var("CODEX_BINARY").unwrap_or_else(|_| "codex".to_string())
}

/// Locate the `scripts/codex-headless.sh` wrapper, walking up from the
/// current directory and the executable directory. Returns None if not
/// found — the caller falls back to invoking `codex` directly.
fn locate_helper_script() -> Option<PathBuf> {
    let candidates = [
        std::env::current_dir().ok(),
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(PathBuf::from)),
    ];
    for start in candidates.into_iter().flatten() {
        let mut dir = start;
        for _ in 0..6 {
            let candidate = dir.join("scripts/codex-headless.sh");
            if candidate.exists() {
                return Some(candidate);
            }
            let parent = match dir.parent() {
                Some(p) => p.to_path_buf(),
                None => break,
            };
            if parent == dir {
                break;
            }
            dir = parent;
        }
    }
    None
}

/// Best-effort check that the `codex` binary is invocable. Returns the
/// reported version string on success.
pub fn probe() -> Result<String, String> {
    let out = Command::new(codex_binary())
        .arg("--version")
        .output()
        .map_err(|e| format!("codex binary not invocable: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "codex --version exited with {}",
            out.status.code().unwrap_or(-1)
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Run `codex exec` headlessly with `prompt` on stdin. Returns the
/// last assistant message extracted from `--output-last-message`.
pub fn run_headless(prompt: &str, opts: CodexOptions) -> Result<CodexResult, String> {
    if prompt.trim().is_empty() {
        return Err("prompt is empty".to_string());
    }

    // No DEFAULT_MODEL — when not specified, defer to whatever codex
    // chooses (currently gpt-5.5). Setting an unknown model causes
    // codex to hang waiting for a response that never arrives.
    let model = opts
        .model
        .or_else(|| std::env::var("CODEX_MODEL").ok())
        .map(|m| m.trim().to_string())
        .filter(|m| !m.is_empty());
    let sandbox = opts.sandbox.unwrap_or_else(|| "read-only".to_string());
    let timeout = Duration::from_millis(opts.timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS));
    let tmp_dir = tempfile::tempdir().map_err(|e| format!("tempdir failed: {e}"))?;
    let output_file = tmp_dir.path().join("last-message.txt");
    // Default cwd is the *isolated* tempdir. Codex slurps the cwd as project
    // context, so pointing it at a source tree blows the token budget into
    // the millions (1.15M observed when cwd = app/src-tauri). The tempdir
    // is empty → codex sees no project, completes a one-shot prompt in
    // single-digit seconds.
    let cwd = opts.cwd.unwrap_or_else(|| {
        std::env::var("CODEX_CWD")
            .ok()
            .map(PathBuf::from)
            .unwrap_or_else(|| tmp_dir.path().to_path_buf())
    });

    let started = std::time::Instant::now();
    // Prefer the helper script which runs codex under `env -i` + `setsid`
    // so none of the Tauri host's polluted environment / inherited FDs
    // can reach codex. Direct spawning from this process deadlocks codex
    // in a futex during startup (only when the launcher is the Tauri
    // main thread). The script is a thin shim that solves both at once.
    let helper = locate_helper_script();
    let mut cmd: Command;
    if let Some(script) = helper.as_ref() {
        cmd = Command::new("bash");
        cmd.arg(script)
            .arg(prompt)
            .arg(&output_file)
            .env("CODEX_CWD", &cwd);
        if let Some(m) = model.as_ref() {
            cmd.env("CODEX_MODEL", m);
        }
    } else {
        // Fallback: invoke codex directly with all the flags. Used only
        // when the helper script is missing (release bundles should ship
        // the script alongside the binary).
        cmd = Command::new(codex_binary());
        cmd.arg("exec")
            .arg("--sandbox")
            .arg(&sandbox)
            .arg("--skip-git-repo-check")
            .arg("--ignore-user-config")
            .arg("--ignore-rules")
            .arg("--cd")
            .arg(&cwd)
            .arg("--output-last-message")
            .arg(&output_file);
        if let Some(m) = model.as_ref() {
            cmd.arg("--model").arg(m);
        }
        cmd.arg(prompt);
    }
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // Close all inherited FDs above stdio AND detach into a fresh session.
    // The Tauri host has hundreds of CEF cache files open without
    // FD_CLOEXEC, and its CEF network namespace / signal mask leak into the
    // child, so codex's node shim stalls (~60-150s) waiting on inherited
    // state. Closing the FDs and creating a new session group brings codex
    // back to its terminal-equivalent ~5-10s cold start.
    #[cfg(unix)]
    unsafe {
        cmd.pre_exec(|| {
            for fd in 3..1024 {
                libc::close(fd);
            }
            // New session → new process group → no controlling terminal.
            // Mirrors `setsid codex …`. Ignore failures (best effort).
            let _ = libc::setsid();
            Ok(())
        });
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("spawn codex failed: {e}"))?;

    let deadline = std::time::Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    let mut stderr_text = String::new();
                    if let Some(mut stderr) = child.stderr.take() {
                        use std::io::Read as _;
                        let _ = stderr.read_to_string(&mut stderr_text);
                    }
                    return Err(format!(
                        "codex exited with {} stderr={}",
                        status.code().unwrap_or(-1),
                        stderr_text.trim()
                    ));
                }
                break;
            }
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    let _ = child.kill();
                    return Err(format!(
                        "codex exec timed out after {}ms",
                        timeout.as_millis()
                    ));
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => return Err(format!("waiting on codex failed: {e}")),
        }
    }

    let content = std::fs::read_to_string(&output_file)
        .map_err(|e| format!("reading codex output file failed: {e}"))?
        .trim()
        .to_string();
    if content.is_empty() {
        return Err("empty codex output".to_string());
    }
    Ok(CodexResult {
        content,
        model: model.unwrap_or_default(),
        elapsed_ms: started.elapsed().as_millis(),
    })
}
