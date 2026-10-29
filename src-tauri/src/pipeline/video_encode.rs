//! Prefer NVENC for video exports, retaining the exact software command for retry.
//! Filters (including libass) remain on the CPU; remuxes never enter this path.
use std::ffi::OsStr;
use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use super::media::run_ffmpeg_progress;

fn nvenc_command(cpu: &Command) -> Option<Command> {
    let args: Vec<_> = cpu.get_args().collect();
    if !args
        .windows(2)
        .any(|a| a == [OsStr::new("-c:v"), OsStr::new("libx264")])
    {
        return None;
    }
    let mut gpu = Command::new(cpu.get_program());
    if let Some(dir) = cpu.get_current_dir() {
        gpu.current_dir(dir);
    }
    for (key, value) in cpu.get_envs() {
        match value {
            Some(v) => {
                gpu.env(key, v);
            }
            None => {
                gpu.env_remove(key);
            }
        }
    }
    let mut index = 0;
    while index < args.len() {
        let arg = args[index];
        if index + 1 < args.len() && arg == "-c:v" && args[index + 1] == "libx264" {
            gpu.args(["-c:v", "h264_nvenc"]);
            index += 2;
        } else if index + 1 < args.len() && arg == "-preset" {
            gpu.args(["-preset", "p5", "-tune", "hq"]);
            index += 2;
        } else if index + 1 < args.len() && arg == "-crf" {
            // CQ and CRF are different scales. CQ 25 is validated against CRF 20
            // on the local 1080p sample; do not cap video to NVENC's default 2 Mb/s.
            gpu.args(["-rc", "vbr", "-cq", "25", "-b:v", "0"]);
            index += 2;
        } else {
            gpu.arg(arg);
            index += 1;
        }
    }
    Some(gpu)
}

fn with_fallback(
    cpu: Command,
    cancel: &AtomicBool,
    run: &mut dyn FnMut(Command) -> Result<(), String>,
) -> Result<(), String> {
    super::stopped(cancel)?;
    if let Some(gpu) = nvenc_command(&cpu) {
        match run(gpu) {
            Ok(()) => return Ok(()),
            Err(error) => {
                // Never restart work after cancellation. -y overwrites only the
                // same partial output on retry; the published result stays intact.
                super::stopped(cancel)?;
                eprintln!("NVENC thất bại, xuất lại bằng CPU: {error}");
            }
        }
    }
    run(cpu)
}

pub(super) fn run(
    cpu: Command,
    total: Option<f64>,
    timeout: Duration,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(f64),
) -> Result<(), String> {
    with_fallback(cpu, cancel, &mut |command| {
        run_ffmpeg_progress(command, total, timeout, cancel, progress)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    fn command() -> Command {
        let mut c = Command::new("ffmpeg");
        c.args([
            "-y",
            "-i",
            "nguồn có dấu.mp4",
            "-vf",
            "ass=filename='text.ass'",
            "-c:v",
            "libx264",
            "-preset",
            "veryfast",
            "-crf",
            "20",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "copy",
            "clip.partial.mp4",
        ])
        .current_dir(".")
        .env("VIDEO_TEST", "kept");
        c
    }

    #[test]
    fn gpu_failure_retries_original_software_command() {
        let c = command();
        let original: Vec<_> = c.get_args().map(|a| a.to_os_string()).collect();
        let mut calls = 0;
        with_fallback(c, &AtomicBool::new(false), &mut |c| {
            calls += 1;
            let args: Vec<_> = c.get_args().map(|a| a.to_os_string()).collect();
            assert_eq!(c.get_current_dir(), Some(std::path::Path::new(".")));
            if calls == 1 {
                assert!(args.contains(&"h264_nvenc".into()));
                assert!(!args.contains(&"-crf".into()));
                assert!(args.contains(&"nguồn có dấu.mp4".into()));
                Err("No NVENC device".into())
            } else {
                assert_eq!(args, original);
                Ok(())
            }
        })
        .unwrap();
        assert_eq!(calls, 2);
    }

    #[test]
    fn success_and_cancel_never_retry() {
        let mut calls = 0;
        with_fallback(command(), &AtomicBool::new(false), &mut |_| {
            calls += 1;
            Ok(())
        })
        .unwrap();
        assert_eq!(calls, 1);
        calls = 0;
        let cancel = AtomicBool::new(false);
        let error = with_fallback(command(), &cancel, &mut |_| {
            calls += 1;
            cancel.store(true, Ordering::Relaxed);
            Err("interrupted".into())
        })
        .unwrap_err();
        assert_eq!(error, "Đã dừng.");
        assert_eq!(calls, 1);
    }

    #[test]
    fn remux_is_unchanged_and_cpu_failure_is_returned() {
        let mut c = Command::new("ffmpeg");
        c.args(["-c:v", "copy"]);
        assert!(nvenc_command(&c).is_none());
        let mut calls = 0;
        let error = with_fallback(command(), &AtomicBool::new(false), &mut |_| {
            calls += 1;
            Err(if calls == 1 { "GPU" } else { "CPU" }.into())
        })
        .unwrap_err();
        assert_eq!(calls, 2);
        assert_eq!(error, "CPU");
    }

    #[test]
    fn real_cpu_retry_overwrites_failed_partial_and_decodes() {
        if crate::program_path("ffmpeg").is_none() {
            return;
        }
        let dir = std::env::temp_dir().join(format!("encode-retry-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("kết quả.partial.mp4");
        let mut c = Command::new("ffmpeg");
        c.args([
            "-v",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=s=320x180:r=10:d=1",
            "-c:v",
            "libx264",
            "-preset",
            "veryfast",
            "-crf",
            "20",
            "-pix_fmt",
            "yuv420p",
        ])
        .arg(&out);
        let cancel = AtomicBool::new(false);
        let mut calls = 0;
        with_fallback(c, &cancel, &mut |mut c| {
            calls += 1;
            if calls == 1 {
                std::fs::write(&out, b"failed GPU output").unwrap();
                // Force a real encoder failure even on machines with a working GPU.
                let args: Vec<_> = c.get_args().map(|a| a.to_os_string()).collect();
                let mut failed = Command::new(c.get_program());
                failed
                    .args(&args[..args.len() - 1])
                    .args(["-gpu", "99999"])
                    .arg(&out);
                c = failed;
            }
            run_ffmpeg_progress(c, None, Duration::from_secs(30), &cancel, &mut |_| {})
        })
        .unwrap();
        assert_eq!(calls, 2);
        let decoded = Command::new("ffmpeg")
            .args(["-v", "error", "-i"])
            .arg(&out)
            .args(["-f", "null", "-"])
            .output()
            .unwrap();
        assert!(
            decoded.status.success(),
            "{}",
            String::from_utf8_lossy(&decoded.stderr)
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
