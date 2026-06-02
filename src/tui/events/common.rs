use anyhow::Result;

pub(crate) fn copy_to_clipboard(text: &str) -> Result<()> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    if cfg!(target_os = "macos") {
        let mut child = Command::new("pbcopy")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(text.as_bytes())?;
        }
        let status = child.wait()?;
        if !status.success() {
            anyhow::bail!("pbcopy failed");
        }
        Ok(())
    } else if cfg!(target_os = "windows") {
        let mut child = Command::new("clip")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(text.as_bytes())?;
        }
        let status = child.wait()?;
        if !status.success() {
            anyhow::bail!("clip failed");
        }
        Ok(())
    } else {
        // Linux / Unix
        let is_wayland = std::env::var("WAYLAND_DISPLAY").is_ok();
        let mut tried_wl = false;

        if is_wayland {
            if let Ok(true) = try_copy_wl(text) {
                return Ok(());
            }
            tried_wl = true;
        }

        if let Ok(true) = try_copy_x11(text) {
            return Ok(());
        }

        if !tried_wl && let Ok(true) = try_copy_wl(text) {
            return Ok(());
        }

        anyhow::bail!("No working clipboard tool found (tried wl-copy, xclip, xsel)")
    }
}

fn try_copy_wl(text: &str) -> Result<bool> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let mut child = match Command::new("wl-copy")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e.into()),
    };

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(text.as_bytes())?;
    }

    let status = child.wait()?;
    Ok(status.success())
}

fn try_copy_x11(text: &str) -> Result<bool> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    // Try xclip
    let child_res = Command::new("xclip")
        .args(["-selection", "clipboard"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();

    match child_res {
        Ok(mut child) => {
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(text.as_bytes())?;
            }
            let status = child.wait()?;
            if status.success() {
                return Ok(true);
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }

    // Try xsel
    let child_res = Command::new("xsel")
        .args(["--clipboard", "--input"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();

    match child_res {
        Ok(mut child) => {
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(text.as_bytes())?;
            }
            let status = child.wait()?;
            if status.success() {
                return Ok(true);
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }

    Ok(false)
}

pub(crate) fn read_from_clipboard() -> Result<String> {
    use std::process::Command;

    if cfg!(target_os = "macos") {
        let output = Command::new("pbpaste")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .output()?;
        if !output.status.success() {
            anyhow::bail!("pbpaste failed");
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else if cfg!(target_os = "windows") {
        let output = Command::new("powershell.exe")
            .args(["-Command", "Get-Clipboard"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .output()?;
        if !output.status.success() {
            anyhow::bail!("powershell Get-Clipboard failed");
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    } else {
        // Linux / Unix
        let is_wayland = std::env::var("WAYLAND_DISPLAY").is_ok();
        let mut tried_wl = false;

        if is_wayland {
            if let Ok(Some(text)) = try_paste_wl() {
                return Ok(text);
            }
            tried_wl = true;
        }

        if let Ok(Some(text)) = try_paste_x11() {
            return Ok(text);
        }

        if !tried_wl && let Ok(Some(text)) = try_paste_wl() {
            return Ok(text);
        }

        anyhow::bail!("No working clipboard tool found for pasting")
    }
}

fn try_paste_wl() -> Result<Option<String>> {
    use std::process::{Command, Stdio};

    let output_res = Command::new("wl-paste")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output();

    match output_res {
        Ok(output) => {
            if output.status.success() {
                Ok(Some(String::from_utf8_lossy(&output.stdout).into_owned()))
            } else {
                Ok(None)
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn try_paste_x11() -> Result<Option<String>> {
    use std::process::{Command, Stdio};

    // Try xclip
    let output_res = Command::new("xclip")
        .args(["-o", "-selection", "clipboard"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output();

    match output_res {
        Ok(output) => {
            if output.status.success() {
                return Ok(Some(String::from_utf8_lossy(&output.stdout).into_owned()));
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }

    // Try xsel
    let output_res = Command::new("xsel")
        .args(["--clipboard", "--output"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output();

    match output_res {
        Ok(output) => {
            if output.status.success() {
                return Ok(Some(String::from_utf8_lossy(&output.stdout).into_owned()));
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }

    Ok(None)
}
