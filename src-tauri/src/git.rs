use serde::Serialize;
use std::process::Command;

#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct GitFile {
    pub code: String,
    pub path: String,
}

#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct GitStatus {
    pub is_repo: bool,
    pub root: String,
    pub branch: String,
    pub upstream: String,
    pub ahead: u32,
    pub behind: u32,
    pub remote_url: String,
    pub files: Vec<GitFile>,
}

pub fn run(dir: &str, args: &[&str]) -> Result<String, String> {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(dir).args(["-c", "core.quotepath=off"]).args(args);
    cmd.env("GIT_TERMINAL_PROMPT", "0");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let out = cmd.output().map_err(|e| format!("git nicht ausführbar: {e}"))?;
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    if out.status.success() {
        Ok(format!("{stdout}{stderr}").trim().to_string())
    } else {
        Err(if stderr.trim().is_empty() { stdout } else { stderr }.trim().to_string())
    }
}

pub fn status(dir: &str) -> GitStatus {
    let Ok(root) = run(dir, &["rev-parse", "--show-toplevel"]) else {
        return GitStatus::default();
    };
    let mut st = GitStatus { is_repo: true, root, ..Default::default() };
    st.remote_url = run(dir, &["remote", "get-url", "origin"]).unwrap_or_default();
    let Ok(out) = run(dir, &["status", "--porcelain=v1", "-b", "--untracked-files=all", "--", "."]) else {
        return st;
    };
    for line in out.lines() {
        if let Some(head) = line.strip_prefix("## ") {
            let (names, counts) = match head.split_once(" [") {
                Some((n, c)) => (n, c.trim_end_matches(']')),
                None => (head, ""),
            };
            match names.split_once("...") {
                Some((b, u)) => {
                    st.branch = b.to_string();
                    st.upstream = u.to_string();
                }
                None => st.branch = names.trim_start_matches("No commits yet on ").to_string(),
            }
            for part in counts.split(", ") {
                if let Some(n) = part.strip_prefix("ahead ") {
                    st.ahead = n.parse().unwrap_or(0);
                } else if let Some(n) = part.strip_prefix("behind ") {
                    st.behind = n.parse().unwrap_or(0);
                }
            }
        } else if line.len() > 3 {
            st.files.push(GitFile { code: line[..2].to_string(), path: line[3..].trim_matches('"').to_string() });
        }
    }
    st
}

pub fn commit(dir: &str, message: &str) -> Result<String, String> {
    if message.trim().is_empty() {
        return Err("Commit-Nachricht fehlt".into());
    }
    run(dir, &["add", "-A", "--", "."])?;
    run(dir, &["commit", "-m", message.trim()])
}
