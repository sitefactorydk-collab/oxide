//! Small process helpers: no console window, bounded output capture, ending a process tree.

use std::process::Stdio;

use tokio::io::AsyncReadExt;

/// Stops a console window from flashing up (Windows); on Unix puts the child in its own
/// process group so [`kill_tree`] reaches what it starts.
pub(crate) fn prepare(cmd: &mut tokio::process::Command) {
    #[cfg(windows)]
    {
        // CREATE_NO_WINDOW
        cmd.creation_flags(0x0800_0000);
    }
    #[cfg(unix)]
    {
        cmd.process_group(0);
    }
    cmd.kill_on_drop(true);
}

/// Reads a stream to its end, keeping at most `cap` bytes (the rest is drained).
pub(crate) async fn capture<R: tokio::io::AsyncRead + Unpin>(
    r: Option<&mut R>,
    cap: usize,
) -> Vec<u8> {
    let Some(r) = r else { return Vec::new() };
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        match r.read(&mut chunk).await {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                if buf.len() < cap {
                    let take = n.min(cap - buf.len());
                    buf.extend_from_slice(&chunk[..take]);
                }
            }
        }
    }
    buf
}

/// Ends a process and what it started.
pub(crate) async fn kill_tree(pid: Option<u32>) {
    let Some(pid) = pid else { return };
    if cfg!(windows) {
        let mut k = tokio::process::Command::new("taskkill");
        k.args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            k.creation_flags(0x0800_0000);
        }
        let _ = k.status().await;
    } else {
        // The child leads its own process group (see `prepare`): `-pid` is the whole group.
        let _ = tokio::process::Command::new("kill")
            .args(["-9", "--", &format!("-{pid}")])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;
    }
}
