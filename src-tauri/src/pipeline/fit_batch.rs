use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use crate::timeline::Placement;

use super::assemble::fit_filter;
use super::media::run_tool;

// Bound input count, RAM and Windows command-line length. DSP stays identical
// to the old one-process-per-line path; only process startup is shared.
pub(super) const FIT_BATCH_LINES: usize = 8;
static NEXT_BATCH: AtomicU64 = AtomicU64::new(0);

pub(super) struct FitBatch {
    dir: PathBuf,
    pub paths: Vec<PathBuf>,
}

impl FitBatch {
    pub fn render(
        parent: &Path,
        sources: &[PathBuf],
        placements: &[Placement],
        cancel: &AtomicBool,
    ) -> Result<Self, String> {
        if sources.is_empty()
            || sources.len() > FIT_BATCH_LINES
            || sources.len() != placements.len()
        {
            return Err("Số câu căn tiếng không hợp lệ.".into());
        }
        super::stopped(cancel)?;
        let dir = loop {
            let id = NEXT_BATCH.fetch_add(1, Ordering::Relaxed);
            let path = parent.join(format!(".fit-{}-{id}", std::process::id()));
            match std::fs::create_dir(&path) {
                Ok(()) => break path,
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(format!("Không tạo được folder căn tiếng: {error}")),
            }
        };
        let batch = Self {
            paths: (0..sources.len())
                .map(|index| dir.join(format!("{index}.wav")))
                .collect(),
            dir,
        };
        let mut command = Command::new("ffmpeg");
        command.args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-filter_complex_threads",
            "1",
        ]);
        for source in sources {
            command.args(["-threads", "1", "-i"]).arg(source);
        }
        let graph = placements
            .iter()
            .enumerate()
            .map(|(index, place)| {
                format!(
                    "[{index}:a]{}[fit{index}]",
                    fit_filter(place.speed, place.keep)
                )
            })
            .collect::<Vec<_>>()
            .join(";");
        command.arg("-filter_complex").arg(graph);
        for (index, path) in batch.paths.iter().enumerate() {
            command
                .arg("-map")
                .arg(format!("[fit{index}]"))
                .args([
                    "-ac",
                    "1",
                    "-ar",
                    "22050",
                    "-c:a",
                    "pcm_s16le",
                    "-threads",
                    "1",
                ])
                .arg(path);
        }
        run_tool(
            command,
            Duration::from_secs(60 * sources.len() as u64),
            cancel,
        )?;
        Ok(batch)
    }
}

impl Drop for FitBatch {
    fn drop(&mut self) {
        for path in &self.paths {
            let _ = std::fs::remove_file(path);
        }
        let _ = std::fs::remove_dir(&self.dir);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wav::{read_wav, write_wav, Pcm};

    #[test]
    fn batched_fit_matches_individual_filters_and_cleans_scratch() {
        if crate::program_path("ffmpeg").is_none() {
            return;
        }
        let dir = std::env::temp_dir().join(format!("fit-batch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("nguồn có dấu.wav");
        let samples = (0..66150)
            .map(|n| ((n as f64 * 0.13).sin() * 12000.0) as i16)
            .collect();
        write_wav(
            &source,
            &Pcm {
                rate: 22050,
                samples,
            },
        )
        .unwrap();
        let placements = [
            Placement {
                start: 0.0,
                speed: 1.0,
                keep: None,
                played: 3.0,
            },
            Placement {
                start: 1.0,
                speed: 1.08,
                keep: None,
                played: 3.0 / 1.08,
            },
            Placement {
                start: 2.0,
                speed: 1.4,
                keep: Some(1.0),
                played: 1.0,
            },
        ];
        let sources = vec![source.clone(); placements.len()];
        let cancel = AtomicBool::new(false);
        let batch = FitBatch::render(&dir, &sources, &placements, &cancel).unwrap();
        let scratch = batch.dir.clone();
        for (index, place) in placements.iter().enumerate() {
            let old = dir.join("old.wav");
            let mut command = Command::new("ffmpeg");
            command
                .args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
                .arg(&source)
                .arg("-af")
                .arg(fit_filter(place.speed, place.keep))
                .args(["-ac", "1", "-ar", "22050", "-c:a", "pcm_s16le"])
                .arg(&old);
            run_tool(command, Duration::from_secs(60), &cancel).unwrap();
            let before = read_wav(&old).unwrap();
            let after = read_wav(&batch.paths[index]).unwrap();
            assert_eq!(before.rate, after.rate);
            assert_eq!(before.samples, after.samples, "line {index}");
            std::fs::remove_file(old).unwrap();
        }
        drop(batch);
        assert!(!scratch.exists());
        // A failed process also removes any partial outputs and leaves inputs intact.
        assert!(
            FitBatch::render(&dir, &[dir.join("missing.wav")], &placements[..1], &cancel).is_err()
        );
        cancel.store(true, Ordering::Relaxed);
        assert!(FitBatch::render(&dir, &sources, &placements, &cancel).is_err());
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_file(source).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }

    #[test]
    #[ignore = "set DUB_BENCH_JOB to a real job folder; reads sources without changing them"]
    fn benchmark_real_job_fit_and_merge() {
        use crate::job::{read_json, Job, TranslatedLine};
        use crate::timeline::{place_all, Slot};
        use crate::wav::mix_at;
        use std::time::Instant;

        let root = PathBuf::from(std::env::var("DUB_BENCH_JOB").expect("DUB_BENCH_JOB"));
        let job = Job::reopen(&root).unwrap();
        let lines: Vec<TranslatedLine> = read_json(&root.join("translation.json")).unwrap();
        let mut slots = Vec::new();
        let mut sources = Vec::new();
        for (index, line) in lines.iter().take(32).enumerate() {
            let source = root.join("lines").join(format!("{:03}.wav", index + 1));
            let pcm = read_wav(&source).unwrap();
            slots.push(Slot {
                start: line.start,
                end: line.end,
                wav_secs: pcm.samples.len() as f64 / f64::from(pcm.rate),
            });
            sources.push(source);
        }
        assert!(!sources.is_empty());
        let total = slots.last().unwrap().end;
        let placements = place_all(&slots, total);
        let dir = std::env::temp_dir().join(format!("real-fit-bench-{}", std::process::id()));
        std::fs::create_dir(&dir).unwrap();
        let cancel = AtomicBool::new(false);
        let old_path = dir.join("old.wav");
        let mut expected = Vec::new();
        let started = Instant::now();
        for (source, place) in sources.iter().zip(&placements) {
            let mut command = Command::new("ffmpeg");
            command
                .args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
                .arg(source)
                .arg("-af")
                .arg(fit_filter(place.speed, place.keep))
                .args(["-ac", "1", "-ar", "22050", "-c:a", "pcm_s16le"])
                .arg(&old_path);
            run_tool(command, Duration::from_secs(60), &cancel).unwrap();
            expected.push(read_wav(&old_path).unwrap());
        }
        let serial = started.elapsed();
        std::fs::remove_file(old_path).unwrap();
        let mut canvas = Vec::new();
        let started = Instant::now();
        for start in (0..sources.len()).step_by(FIT_BATCH_LINES) {
            let end = (start + FIT_BATCH_LINES).min(sources.len());
            let batch =
                FitBatch::render(&dir, &sources[start..end], &placements[start..end], &cancel)
                    .unwrap();
            for (offset, path) in batch.paths.iter().enumerate() {
                let pcm = read_wav(path).unwrap();
                assert_eq!(pcm.rate, expected[start + offset].rate);
                assert_eq!(
                    pcm.samples,
                    expected[start + offset].samples,
                    "real line {}",
                    start + offset + 1
                );
                mix_at(
                    &mut canvas,
                    pcm.rate,
                    placements[start + offset].start,
                    &pcm.samples,
                );
            }
        }
        let batched = started.elapsed();
        let wav = dir.join("dub.wav");
        canvas.resize((total * 22050.0).ceil() as usize, 0);
        write_wav(
            &wav,
            &Pcm {
                rate: 22050,
                samples: canvas,
            },
        )
        .unwrap();
        let video = dir.join("sample.mp4");
        let mut merge = Command::new("ffmpeg");
        merge
            .args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
            .arg(&job.video)
            .arg("-i")
            .arg(&wav)
            .args([
                "-map", "0:v:0", "-map", "1:a:0", "-c:v", "copy", "-c:a", "aac", "-t",
            ])
            .arg(format!("{total:.3}"))
            .arg(&video);
        run_tool(merge, Duration::from_secs(120), &cancel).unwrap();
        let duration = super::super::media::media_duration(&video, &cancel).unwrap();
        assert!((duration - total).abs() < 2.0);
        let mut decode = Command::new("ffmpeg");
        decode
            .args(["-hide_banner", "-loglevel", "error", "-i"])
            .arg(&video)
            .args(["-f", "null", "-"]);
        run_tool(decode, Duration::from_secs(120), &cancel).unwrap();
        println!("REAL_FIT_BENCH lines={} serial={:.3}s batched={:.3}s speedup={:.2}x pcm=identical video_secs={duration:.3} preview={}", sources.len(), serial.as_secs_f64(), batched.as_secs_f64(), serial.as_secs_f64() / batched.as_secs_f64(), video.display());
        std::fs::remove_file(wav).unwrap();
        // Keep the decoded, verified preview so it can be inspected by the user.
    }
}
