use super::assemble::{merge_picture, picture_has_audio, subtitle_filter, write_video_output};
use super::fit_batch::{FitBatch, FIT_BATCH_LINES};
use super::media::{media_duration, write_srt};
use crate::job::{read_json, Job, TranslatedLine};
use crate::timeline::place_all;
use crate::wav::{mix_at, read_wav, resample_linear, write_wav, Pcm};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

struct Scratch {
    dir: PathBuf,
    keep_video: bool,
}
impl Drop for Scratch {
    fn drop(&mut self) {
        for name in ["voice.wav", "sample.srt"] {
            let _ = std::fs::remove_file(self.dir.join(name));
        }
        if !self.keep_video {
            let _ = std::fs::remove_file(self.dir.join("sample.mp4"));
        }
        let _ = std::fs::remove_dir(&self.dir);
    }
}

pub(super) fn voice_excerpt(
    job: &Job,
    lines: &[TranslatedLine],
    start: f64,
    seconds: f64,
    dir: &Path,
    cancel: &AtomicBool,
) -> Result<(PathBuf, Vec<crate::audio_mix::VoiceSpan>), String> {
    let slots = super::demand::speech_slots(job, lines)?;
    let mut rate = 22050;
    for (i, line) in lines.iter().enumerate() {
        super::stopped(cancel)?;
        let required = line.start < start + seconds && line.end > start;
        let pcm = match if slots[i].wav_secs > 0.0 {read_wav(&job.line_wav(i + 1))}else{Err("Chưa có tiếng hợp lệ.".into())} {
            Ok(pcm) => Some(pcm),
            Err(_) if !required => None,
            Err(_) => return Err(format!("Thiếu tiếng câu {} trong đoạn thử. Chuẩn bị lại đoạn này.", i+1)),
        };
        if let Some(pcm)=&pcm {rate=pcm.rate;}
    }
    let placements = place_all(&slots, media_duration(&merge_picture(job)?, cancel)?);
    let picked: Vec<_> = placements
        .iter()
        .enumerate()
        .filter(|(_, p)| p.start < start + seconds && p.start + p.played > start)
        .map(|(i, _)| i)
        .collect();
    let mut canvas = vec![0i16; (seconds * f64::from(rate)).ceil() as usize];
    for chunk in picked.chunks(FIT_BATCH_LINES) {
        let sources: Vec<_> = chunk.iter().map(|i| job.line_wav(i + 1)).collect();
        let fit: Vec<_> = chunk.iter().map(|i| placements[*i]).collect();
        let batch = FitBatch::render(dir, &sources, &fit, cancel)?;
        for (i, path) in chunk.iter().zip(&batch.paths) {
            let pcm = read_wav(path)?;
            let samples = if pcm.rate == rate {
                pcm.samples
            } else {
                resample_linear(&pcm.samples, pcm.rate, rate)
            };
            let relative = placements[*i].start - start;
            let skip = ((-relative).max(0.0) * f64::from(rate)).round() as usize;
            if skip < samples.len() {
                mix_at(&mut canvas, rate, relative.max(0.0), &samples[skip..]);
            }
        }
    }
    canvas.truncate((seconds * f64::from(rate)).ceil() as usize);
    let path = dir.join("voice.wav");
    write_wav(
        &path,
        &Pcm {
            rate,
            samples: canvas,
        },
    )?;
    let spans = placements
        .iter()
        .filter(|p| p.start < start + seconds && p.start + p.played > start)
        .map(|p| crate::audio_mix::VoiceSpan {
            start: (p.start - start).max(0.0),
            end: (p.start + p.played - start).min(seconds),
        })
        .collect();
    Ok((path, spans))
}

pub(crate) fn export(
    job: &Job,
    start: f64,
    seconds: f64,
    text_only: bool,
    burn: bool,
    mut render: crate::render::RenderSettings,
    cancel: &AtomicBool,
) -> Result<String, String> {
    if !start.is_finite() || start < 0.0 || !seconds.is_finite() || !(1.0..=10.0).contains(&seconds)
    {
        return Err("Chọn mốc không âm và đoạn thử tối đa 10 giây.".into());
    }
    render.validate()?;
    let picture = merge_picture(job)?;
    let duration = media_duration(&picture, cancel)?;
    let seconds = seconds.min(duration - start);
    if seconds < 0.1 {
        return Err("Mốc xem thử vượt cuối video.".into());
    }
    let lines = crate::timing::read(&job.root)?;
    let saved = crate::render::RenderSettings::read(&job.root)?;
    if !text_only
        && super::demand::selected(&lines,start,start+seconds,true).iter().any(|&i| {
            let l=&lines[i];
            crate::render::spoken_text(&saved, &job.target, &l.translation)
                != crate::render::spoken_text(&render, &job.target, &l.translation)
        })
    {
        return Err(
            "Cách đọc đã đổi. Chạy lại để tạo tiếng các câu bị ảnh hưởng trước khi xem thử.".into(),
        );
    }
    crate::media_assets::stage(job, &mut render)?;
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let parent = job.root.join("export-preview");
    std::fs::create_dir_all(&parent).map_err(|e| e.to_string())?;
    let dir = parent.join(format!(
        "sample-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&dir).map_err(|e| e.to_string())?;
    let mut scratch = Scratch {
        dir: dir.clone(),
        keep_video: false,
    };
    let out = dir.join("sample.mp4");
    let mut command = Command::new("ffmpeg");
    command
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-progress",
            "pipe:1",
            "-nostats",
            "-y",
            "-ss",
            &start.to_string(),
            "-i",
        ])
        .arg(&picture);
    let mut next_input = 1;
    let mut audio_graph = String::new();
    let mut envelope = None;
    if !text_only {
        let (voice, spans) = voice_excerpt(job, &lines, start, seconds, &dir, cancel)?;
        command.arg("-i").arg(voice);
        next_input = 2;
        let original = if render.audio.original > 0.0 && picture_has_audio(&picture) {
            Some(0)
        } else {
            None
        };
        let env_input = if original.is_some() && render.audio.duck {
            envelope = Some(crate::audio_mix::prepare_envelope(
                &dir,
                &render.audio,
                &spans,
                seconds,
            )?);
            command.arg("-i").arg(&envelope.as_ref().unwrap().0);
            next_input += 1;
            Some(2)
        } else {
            None
        };
        audio_graph = render.audio.graph(original, env_input);
    }
    let logo_input = if render.logo.enabled {
        command.arg("-i").arg(&render.logo.path);
        Some(next_input)
    } else {
        None
    };
    let (w, h) = super::captions::probe_dimensions(&picture, cancel)?;
    let (ow, oh) = render.dimensions(w, h);
    let srt = dir.join("sample.srt");
    let overlay = if text_only || burn {
        let visible:Vec<_>=lines.iter().filter(|l|l.start<start+seconds&&l.end>start).cloned().collect();
        write_srt(&srt, &visible)?;
        Some(super::captions::prepare_for(&srt, ow, oh, &render)?)
    } else {
        None
    };
    let tail = overlay
        .as_ref()
        .map(|ass| {
            format!(
                ",setpts=PTS+{start:.6}/TB,{},setpts=PTS-STARTPTS",
                subtitle_filter(ass.0.file_name().unwrap().to_str().unwrap())
            )
        })
        .unwrap_or_default();
    let mut graph = render.graph_with_logo(w, h, &tail, logo_input);
    if !audio_graph.is_empty() {
        graph.push(';');
        graph.push_str(&audio_graph);
    }
    command.args(["-filter_complex", &graph, "-map", "[vout]"]);
    if text_only {
        command.args(["-map", "0:a?"]);
    } else {
        command.args(["-map", "[aout]"]);
    }
    command
        .args([
            "-t",
            &seconds.to_string(),
            "-c:v",
            "libx264",
            "-preset",
            "veryfast",
            "-threads",
            "2",
            "-bf",
            "0",
            "-crf",
            "20",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            "-ar",
            "48000",
            "-ac",
            "2",
            "-movflags",
            "+faststart",
        ])
        .current_dir(&dir);
    write_video_output(command, &out, Some(seconds), cancel, &mut |_| {})?;
    drop(envelope);
    drop(overlay);
    scratch.keep_video = true;
    // Replace only the previous preview published by this command. Unknown files stay intact.
    let marker = job.root.join("export-preview-last.json");
    if let Ok(old) = read_json::<PathBuf>(&marker) {
        if old != out
            && old.file_name().is_some_and(|s| s == "sample.mp4")
            && old.parent().and_then(Path::parent) == Some(parent.as_path())
        {
            let _ = std::fs::remove_file(&old);
            if let Some(dir) = old.parent() {
                let _ = std::fs::remove_dir(dir);
            }
        }
    }
    crate::job::write_json(&marker, &out)?;
    Ok(out.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    #[test]
    fn production_merge_and_excerpt_share_mix_logo_and_source_mask() {
        let dir = std::env::temp_dir().join(format!("production-render-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("input.mp4");
        let encoded = Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "color=c=blue:s=320x180:r=10:d=4",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=4",
                "-c:v",
                "libx264",
                "-preset",
                "ultrafast",
                "-c:a",
                "aac",
            ])
            .arg(&source)
            .output()
            .unwrap();
        assert!(
            encoded.status.success(),
            "{}",
            String::from_utf8_lossy(&encoded.stderr)
        );
        let job = Job::open(&source, &dir).unwrap();
        let lines = vec![
            TranslatedLine {
                start: 0.0,
                end: 1.0,
                source: "Hello".into(),
                translation: "Xin chào".into(),
            },
            TranslatedLine {
                start: 2.0,
                end: 3.0,
                source: "Next".into(),
                translation: "Dòng thứ hai".into(),
            },
        ];
        crate::job::write_json(&job.translation_path(), &lines).unwrap();
        let samples: Vec<i16> = (0..22050)
            .map(|i| ((i as f64 * 330.0 * std::f64::consts::TAU / 22050.0).sin() * 3000.0) as i16)
            .collect();
        for i in 1..=2 {
            write_wav(
                &job.line_wav(i),
                &Pcm {
                    rate: 22050,
                    samples: samples.clone(),
                },
            )
            .unwrap();
        }
        let mut full = vec![0i16; 4 * 22050];
        mix_at(&mut full, 22050, 0.0, &samples);
        mix_at(&mut full, 22050, 2.0, &samples);
        write_wav(
            &job.dub(),
            &Pcm {
                rate: 22050,
                samples: full,
            },
        )
        .unwrap();
        let logo = dir.join("logo.png");
        assert!(Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "color=c=red:s=16x16",
                "-frames:v",
                "1"
            ])
            .arg(&logo)
            .output()
            .unwrap()
            .status
            .success());
        let mut settings = crate::render::RenderSettings::default();
        settings.audio.original = 60.0;
        settings.logo = crate::media_assets::Logo {
            enabled: true,
            path: logo.to_string_lossy().into_owned(),
            sha256: format!("{:x}", Sha256::digest(std::fs::read(&logo).unwrap())),
            opacity: 100.0,
            ..Default::default()
        };
        settings.mask = crate::media_assets::SourceMask {
            enabled: true,
            mode: "solid".into(),
            x: 0.0,
            y: 60.0,
            width: 100.0,
            height: 20.0,
            color: "#00ff00".into(),
        };
        crate::media_assets::stage(&job, &mut settings).unwrap();
        std::fs::remove_file(logo).unwrap();
        crate::job::write_json(&job.root.join("render.json"), &settings).unwrap();
        super::super::assemble::merge_video_file(
            &job,
            &lines,
            &AtomicBool::new(false),
            &mut |_| {},
        )
        .unwrap();
        let sample = PathBuf::from(
            export(
                &job,
                0.5,
                2.0,
                false,
                false,
                settings.clone(),
                &AtomicBool::new(false),
            )
            .unwrap(),
        );
        for path in [job.result(), sample.clone()] {
            let data = Command::new("ffmpeg")
                .args(["-v", "error", "-ss", "0.2", "-i"])
                .arg(&path)
                .args([
                    "-frames:v",
                    "1",
                    "-f",
                    "rawvideo",
                    "-pix_fmt",
                    "rgb24",
                    "pipe:1",
                ])
                .output()
                .unwrap();
            assert!(
                data.status.success(),
                "{}",
                String::from_utf8_lossy(&data.stderr)
            );
            let pixel = |x: usize, y: usize| &data.stdout[(y * 320 + x) * 3..(y * 320 + x) * 3 + 3];
            let red = pixel(278, 30);
            assert!(red[0] > 180 && red[1] < 80, "logo {red:?}");
            let green = pixel(160, 125);
            assert!(green[1] > 180 && green[0] < 80, "mask {green:?}");
            assert!(
                (media_duration(&path, &AtomicBool::new(false)).unwrap()
                    - if path == sample { 2.0 } else { 4.0 })
                .abs()
                    < 0.2
            );
        }
        let packets = |path: &Path| {
            Command::new("ffprobe")
                .args([
                    "-v",
                    "error",
                    "-select_streams",
                    "v:0",
                    "-show_packets",
                    "-show_data_hash",
                    "sha256",
                    "-show_entries",
                    "packet=data_hash",
                    "-of",
                    "csv=p=0",
                ])
                .arg(path)
                .output()
                .unwrap()
                .stdout
        };
        let before_packets = packets(&job.result());
        let mut remixed = settings.clone();
        remixed.audio.original = 30.0;
        crate::render::settle_for(&job, &remixed, false, false).unwrap();
        assert!(job.root.join("frame-cache.json").is_file());
        super::super::assemble::merge_video_file(
            &job,
            &lines,
            &AtomicBool::new(false),
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(
            packets(&job.result()),
            before_packets,
            "changing volume must reuse the encoded video"
        );
        let silent = dir.join("silent.mp4");
        assert!(Command::new("ffmpeg")
            .args(["-v", "error", "-y", "-i"])
            .arg(&source)
            .args(["-an", "-c:v", "copy"])
            .arg(&silent)
            .output()
            .unwrap()
            .status
            .success());
        let silent_job = Job::open(&silent, &dir).unwrap();
        crate::job::write_json(&silent_job.translation_path(), &lines).unwrap();
        for i in 1..=2 {
            std::fs::copy(job.line_wav(i), silent_job.line_wav(i)).unwrap();
        }
        std::fs::copy(job.dub(), silent_job.dub()).unwrap();
        crate::job::write_json(&silent_job.root.join("render.json"), &remixed).unwrap();
        super::super::assemble::merge_video_file(
            &silent_job,
            &lines,
            &AtomicBool::new(false),
            &mut |_| {},
        )
        .unwrap();
        assert!(
            picture_has_audio(&silent_job.result()),
            "a silent source must still receive the dubbed audio"
        );
        settings.mask.mode = "blur".into();
        settings.aspect = "9:16".into();
        let portrait = export(
            &job,
            0.0,
            2.0,
            false,
            true,
            settings,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(
            super::super::captions::probe_dimensions(Path::new(&portrait), &AtomicBool::new(false))
                .unwrap(),
            (180, 320)
        );
        assert!(!sample.exists());
        assert!(export(
            &job,
            0.0,
            2.0,
            false,
            true,
            Default::default(),
            &AtomicBool::new(true)
        )
        .is_err());
        if std::env::var_os("DUB_KEEP_MEDIA_TESTS").is_some() {
            println!("Production review: {}", dir.display());
        } else {
            std::fs::remove_dir_all(dir).unwrap();
        }
    }
}
