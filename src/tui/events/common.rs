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

pub(crate) fn pasted_images_dir() -> Result<std::path::PathBuf> {
    use anyhow::Context;
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("APPDATA").map(std::path::PathBuf::from))
        .or_else(|| {
            std::env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".config"))
        })
        .or_else(|| {
            std::env::var_os("USERPROFILE")
                .map(|home| std::path::PathBuf::from(home).join(".config"))
        })
        .context("could not find HOME, USERPROFILE, APPDATA, or XDG_CONFIG_HOME")?;

    let dir = base.join("darwincode").join("pasted_images");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub(crate) fn uuid_or_timestamp() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| format!("{}_{}", d.as_secs(), d.subsec_nanos()))
        .unwrap_or_else(|_| "temp".to_owned())
}

pub(crate) fn read_image_from_clipboard() -> Result<Option<Vec<u8>>> {
    use std::process::{Command, Stdio};

    if cfg!(target_os = "macos") {
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join(format!("darwin_paste_{}.png", uuid_or_timestamp()));

        let script = format!(
            "try\n\
             set theFile to a reference to (POSIX file \"{}\")\n\
             set pngData to the clipboard as «class PNGf»\n\
             open for access theFile with write permission\n\
             set eof of theFile to 0\n\
             write pngData to theFile\n\
             close access theFile\n\
             on error\n\
             try\n\
             close access theFile\n\
             end try\n\
             end try",
            temp_file.display()
        );

        let _ = Command::new("osascript").args(["-e", &script]).output()?;

        if temp_file.exists() {
            let bytes = std::fs::read(&temp_file)?;
            let _ = std::fs::remove_file(&temp_file);
            return Ok(Some(bytes));
        }
        Ok(None)
    } else if cfg!(target_os = "windows") {
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join(format!("darwin_paste_{}.png", uuid_or_timestamp()));

        let cmd = format!(
            "Add-Type -AssemblyName System.Windows.Forms; \
             if ([System.Windows.Forms.Clipboard]::ContainsImage()) {{ \
                 [System.Windows.Forms.Clipboard]::GetImage().Save('{}', [System.Drawing.Imaging.ImageFormat]::Png) \
             }}",
            temp_file.display()
        );

        let _ = Command::new("powershell.exe")
            .args(["-NoProfile", "-Command", &cmd])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;

        if temp_file.exists() {
            let bytes = std::fs::read(&temp_file)?;
            let _ = std::fs::remove_file(&temp_file);
            return Ok(Some(bytes));
        }
        Ok(None)
    } else {
        // Linux / Unix
        let is_wayland = std::env::var("WAYLAND_DISPLAY").is_ok();
        if is_wayland {
            let output = Command::new("wl-paste")
                .args(["--type", "image/png"])
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .output();
            if let Ok(output) = output
                && output.status.success()
                && !output.stdout.is_empty()
            {
                return Ok(Some(output.stdout));
            }
        }

        // Try xclip
        let output = Command::new("xclip")
            .args(["-selection", "clipboard", "-t", "image/png", "-o"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output();
        if let Ok(output) = output
            && output.status.success()
            && !output.stdout.is_empty()
        {
            return Ok(Some(output.stdout));
        }

        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uuid_or_timestamp() {
        let u1 = uuid_or_timestamp();
        let u2 = uuid_or_timestamp();
        assert_ne!(u1, u2);
    }

    #[test]
    fn test_pasted_images_dir() {
        // Temp home directory to avoid messing with real user config
        std::env::set_var("HOME", std::env::temp_dir());
        let dir = pasted_images_dir().unwrap();
        assert!(dir.exists());
        assert!(dir.ends_with("darwincode/pasted_images"));
    }
}
