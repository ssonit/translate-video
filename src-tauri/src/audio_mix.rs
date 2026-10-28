use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(crate) struct AudioMix {
    pub original: f64,
    pub voice: f64,
    pub duck: bool,
    pub duck_level: f64,
}

impl Default for AudioMix {
    fn default() -> Self {
        Self {
            original: 0.0,
            voice: 100.0,
            duck: true,
            duck_level: 22.0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct VoiceSpan {
    pub start: f64,
    pub end: f64,
}

impl AudioMix {
    pub fn validate(&self) -> Result<(), String> {
        if [
            (self.original, 100.0),
            (self.voice, 200.0),
            (self.duck_level, 100.0),
        ]
        .iter()
        .any(|(v, max)| !v.is_finite() || *v < 0.0 || *v > *max)
        {
            return Err("Âm lượng ngoài phạm vi cho phép.".into());
        }
        Ok(())
    }

    // A 1 kHz control signal avoids FFmpeg expression recursion limits for long videos.
    pub fn envelope(&self, spans: &[VoiceSpan], seconds: f64) -> crate::wav::Pcm {
        let mut samples = vec![i16::MAX; (seconds.max(0.0) * 1000.0).ceil() as usize];
        let low = self.duck_level / 100.0;
        for span in spans.iter().filter(|s| {
            s.start.is_finite() && s.end.is_finite() && s.end > s.start && s.start >= 0.0
        }) {
            let start = ((span.start - 0.08).max(0.0) * 1000.0).floor() as usize;
            let end = ((span.end + 0.16) * 1000.0).ceil() as usize;
            let until = end.min(samples.len());
            for i in start.min(until)..until {
                let t = i as f64 / 1000.0;
                let gain = if t < span.start {
                    1.0 - (1.0 - low) * (t - (span.start - 0.08)) / 0.08
                } else if t < span.end {
                    low
                } else {
                    low + (1.0 - low) * (t - span.end) / 0.16
                };
                samples[i] =
                    samples[i].min((gain.clamp(low, 1.0) * f64::from(i16::MAX)).round() as i16);
            }
        }
        crate::wav::Pcm {
            rate: 1000,
            samples,
        }
    }

    pub fn graph(&self, source_input: Option<usize>, envelope_input: Option<usize>) -> String {
        let voice = self.voice / 100.0;
        let original = self.original / 100.0;
        let limit = "alimiter=limit=0.95:level=false:latency=true";
        if let Some(source) = source_input.filter(|_| self.original > 0.0) {
            let mut graph=format!("[{source}:a:0]aformat=sample_rates=48000:channel_layouts=stereo,volume={original:.5}[source];");
            if let Some(env) = envelope_input {
                graph.push_str(&format!("[{env}:a:0]aformat=sample_rates=48000:channel_layouts=stereo[envelope];[source][envelope]amultiply[original];"));
            } else {
                graph.push_str("[source]anull[original];");
            }
            graph.push_str(&format!("[1:a:0]aformat=sample_rates=48000:channel_layouts=stereo,volume={voice:.5}[voice];[original][voice]amix=inputs=2:duration=longest:normalize=0,{limit}[aout]"));
            graph
        } else {
            format!("[1:a:0]aformat=sample_rates=48000:channel_layouts=stereo,volume={voice:.5},{limit}[aout]")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_levels_are_rejected_and_envelope_uses_actual_spans() {
        assert!(AudioMix {
            voice: f64::NAN,
            ..Default::default()
        }
        .validate()
        .is_err());
        let mix = AudioMix {
            original: 80.0,
            ..Default::default()
        };
        let pcm = mix.envelope(
            &[VoiceSpan {
                start: 2.0,
                end: 4.0,
            }],
            6.0,
        );
        assert_eq!(pcm.samples[1000], i16::MAX);
        assert!(pcm.samples[3000] < 8000);
        assert_eq!(pcm.samples[5000], i16::MAX);
        assert!(pcm.samples[1950] > pcm.samples[2000]);
        assert!(pcm.samples[4050] > pcm.samples[4000]);
        assert!(!mix.graph(None, None).contains("[2:a"));
    }

    #[test]
    fn real_mix_ducks_only_fitted_spans_and_keeps_duration() {
        let dir = std::env::temp_dir().join(format!("dub-mix-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mix = AudioMix {
            original: 100.0,
            voice: 0.0,
            duck: true,
            duck_level: 20.0,
        };
        let env = prepare_envelope(
            &dir,
            &mix,
            &[VoiceSpan {
                start: 1.0,
                end: 2.0,
            }],
            3.0,
        )
        .unwrap();
        let graph = mix
            .graph(Some(0), Some(2))
            .replace("[aout]", "[mixed];[mixed]pan=mono|c0=c0[aout]");
        let out = dir.join("test.wav");
        let encoded = std::process::Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=3",
                "-f",
                "lavfi",
                "-i",
                "anullsrc=r=22050:cl=mono:d=3",
                "-i",
            ])
            .arg(&env.0)
            .args([
                "-filter_complex",
                &graph,
                "-map",
                "[aout]",
                "-c:a",
                "pcm_s16le",
            ])
            .arg(&out)
            .output()
            .unwrap();
        assert!(
            encoded.status.success(),
            "{}",
            String::from_utf8_lossy(&encoded.stderr)
        );
        let pcm = crate::wav::read_wav(&out).unwrap();
        let rms = |a: f64, b: f64| {
            let slice = &pcm.samples
                [(a * f64::from(pcm.rate)) as usize..(b * f64::from(pcm.rate)) as usize];
            (slice.iter().map(|v| f64::from(*v).powi(2)).sum::<f64>() / slice.len() as f64).sqrt()
        };
        let before = rms(0.2, 0.7);
        let inside = rms(1.2, 1.7);
        let after = rms(2.3, 2.8);
        assert!(
            (inside / before - 0.2).abs() < 0.03,
            "duck ratio {}",
            inside / before
        );
        assert!((after / before - 1.0).abs() < 0.03);
        assert!((pcm.samples.len() as f64 / f64::from(pcm.rate) - 3.0).abs() < 0.05);
        drop(env);
        std::fs::remove_dir_all(dir).unwrap();
    }
}

// Removes only a uniquely-created control WAV, including cancellation/error paths.
pub(crate) struct EnvelopeFile(pub std::path::PathBuf);
impl Drop for EnvelopeFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
pub(crate) fn prepare_envelope(
    root: &std::path::Path,
    mix: &AudioMix,
    spans: &[VoiceSpan],
    seconds: f64,
) -> Result<EnvelopeFile, String> {
    use std::io::Write;
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let path = root.join(format!(
        "mix-envelope-{}-{}.wav",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|e| e.to_string())?;
    let guard = EnvelopeFile(path);
    file.write_all(&crate::wav::encode_wav(&mix.envelope(spans, seconds)))
        .map_err(|e| e.to_string())?;
    Ok(guard)
}
