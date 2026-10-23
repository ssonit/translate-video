use std::path::Path;

pub(crate) struct Pcm {
    pub rate: u32,
    pub samples: Vec<i16>,
}

pub(crate) fn parse_wav(bytes: &[u8]) -> Result<Pcm, String> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("Không phải file WAV.".into());
    }
    let mut pos = 12;
    let mut rate = None;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let size = u32::from_le_bytes([
            bytes[pos + 4],
            bytes[pos + 5],
            bytes[pos + 6],
            bytes[pos + 7],
        ]) as usize;
        let body = pos + 8;
        if id == b"fmt " {
            if body + 16 > bytes.len() {
                return Err("WAV bị cụt.".into());
            }
            let tag = u16::from_le_bytes([bytes[body], bytes[body + 1]]);
            let channels = u16::from_le_bytes([bytes[body + 2], bytes[body + 3]]);
            let sample_rate = u32::from_le_bytes([
                bytes[body + 4],
                bytes[body + 5],
                bytes[body + 6],
                bytes[body + 7],
            ]);
            let bits = u16::from_le_bytes([bytes[body + 14], bytes[body + 15]]);
            if tag != 1 || channels != 1 || bits != 16 {
                return Err("WAV phải là PCM 16-bit mono.".into());
            }
            rate = Some(sample_rate);
        } else if id == b"data" {
            let rate = rate.ok_or("WAV thiếu khối fmt.")?;
            let end = body.saturating_add(size).min(bytes.len());
            let samples = bytes[body..end]
                .chunks_exact(2)
                .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
                .collect();
            return Ok(Pcm { rate, samples });
        }
        pos = body.saturating_add(size).saturating_add(size & 1);
    }
    Err("WAV không có khối data.".into())
}

pub(crate) fn encode_wav(pcm: &Pcm) -> Vec<u8> {
    let data_len = (pcm.samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&pcm.rate.to_le_bytes());
    out.extend_from_slice(&(pcm.rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for sample in &pcm.samples {
        out.extend_from_slice(&sample.to_le_bytes());
    }
    out
}

pub(crate) fn read_wav(path: &Path) -> Result<Pcm, String> {
    let bytes = std::fs::read(path).map_err(|_| format!("Không đọc được {}.", path.display()))?;
    parse_wav(&bytes)
        .map_err(|error| format!("{} ({}).", error.trim_end_matches('.'), path.display()))
}

pub(crate) fn write_wav(path: &Path, pcm: &Pcm) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, encode_wav(pcm)).map_err(|error| error.to_string())?;
    std::fs::rename(&tmp, path).map_err(|error| error.to_string())
}

// Audio nghe truyện: nối các câu thành một mạch. Hơi ngắn giữa câu, nghỉ lâu hơn khi
// video gốc ngắt một đoạn. Không giữ khoảng im của hình.
pub(crate) const STORY_BREATH: f64 = 0.12;
pub(crate) const STORY_REST: f64 = 0.45;
pub(crate) const STORY_PARAGRAPH_GAP: f64 = 0.65;
pub(crate) const STORY_FADE: f64 = 0.02;
// Đoạn sau không được đè lên từ cuối của đoạn trước.
pub(crate) const PASSAGE_TAIL: f64 = 0.28;
const TRIM_PAD: f64 = 0.008;
const SILENCE: u16 = 200;

pub(crate) struct StoryPiece {
    pub gap_before: f64,
    pub samples: Vec<i16>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct StorySpan {
    pub start: f64,
    pub end: f64,
}

struct Planned {
    pause: usize,
    samples: Vec<i16>,
}

fn secs_to_samples(secs: f64, rate: u32) -> usize {
    if rate == 0 || !secs.is_finite() || secs <= 0.0 {
        0
    } else {
        (secs * f64::from(rate)).round() as usize
    }
}

fn trim_silence(samples: &[i16], rate: u32) -> &[i16] {
    let Some(first) = samples
        .iter()
        .position(|sample| sample.unsigned_abs() > SILENCE)
    else {
        return &[];
    };
    let last = samples
        .iter()
        .rposition(|sample| sample.unsigned_abs() > SILENCE)
        .unwrap_or(first);
    let pad = secs_to_samples(TRIM_PAD, rate);
    let start = first.saturating_sub(pad);
    let end = (last + 1 + pad).min(samples.len());
    &samples[start..end]
}

fn fade_out(clip: &mut [i16], fade: usize) {
    let fade = fade.min(clip.len());
    if fade == 0 {
        return;
    }
    for index in 0..fade {
        let gain = index as f64 / fade as f64;
        let tail = clip.len() - 1 - index;
        clip[tail] = (f64::from(clip[tail]) * gain).round() as i16;
    }
}

fn fade_in(clip: &mut [i16], fade: usize) {
    let fade = fade.min(clip.len());
    if fade == 0 {
        return;
    }
    for index in 0..fade {
        let gain = index as f64 / fade as f64;
        clip[index] = (f64::from(clip[index]) * gain).round() as i16;
    }
}

fn fade_edges(clip: &mut [i16], fade: usize) {
    let fade = fade.min(clip.len() / 2);
    if fade == 0 {
        return;
    }
    for index in 0..fade {
        let gain = index as f64 / fade as f64;
        let scale = |sample: i16| (f64::from(sample) * gain).round() as i16;
        clip[index] = scale(clip[index]);
        let tail = clip.len() - 1 - index;
        clip[tail] = scale(clip[tail]);
    }
}

fn plan_story(rate: u32, pieces: &[StoryPiece]) -> Vec<Planned> {
    let fade = secs_to_samples(STORY_FADE, rate);
    let mut placed = false;
    let mut planned = Vec::with_capacity(pieces.len());
    for piece in pieces {
        let mut samples = trim_silence(&piece.samples, rate).to_vec();
        let pause = if samples.is_empty() || !placed {
            0
        } else if piece.gap_before >= STORY_PARAGRAPH_GAP {
            secs_to_samples(STORY_REST, rate)
        } else {
            secs_to_samples(STORY_BREATH, rate)
        };
        if !samples.is_empty() {
            fade_edges(&mut samples, fade);
            placed = true;
        }
        planned.push(Planned { pause, samples });
    }
    planned
}

pub(crate) fn story_spans(rate: u32, pieces: &[StoryPiece]) -> Vec<StorySpan> {
    let rate_f = f64::from(rate.max(1));
    let mut cursor = 0usize;
    plan_story(rate, pieces)
        .into_iter()
        .map(|clip| {
            cursor += clip.pause;
            let start = cursor as f64 / rate_f;
            cursor += clip.samples.len();
            StorySpan {
                start,
                end: cursor as f64 / rate_f,
            }
        })
        .collect()
}

pub(crate) struct PassageClip {
    pub rest_before: bool,
    pub samples: Vec<i16>,
}

struct PlacedPassage {
    gap: usize,
    samples: Vec<i16>,
}

fn place_passages(rate: u32, clips: &[PassageClip]) -> Vec<PlacedPassage> {
    let fade = secs_to_samples(STORY_FADE, rate);
    let mut pending_rest = false;
    let mut heard = false;
    let mut placed: Vec<PlacedPassage> = Vec::new();
    for clip in clips {
        let mut samples = trim_silence(&clip.samples, rate).to_vec();
        if samples.is_empty() {
            if heard && clip.rest_before {
                pending_rest = true;
            }
            continue;
        }
        let rest = heard && (clip.rest_before || pending_rest);
        pending_rest = false;
        let gap = if !heard {
            0
        } else if rest {
            secs_to_samples(STORY_REST, rate)
        } else {
            secs_to_samples(PASSAGE_TAIL, rate)
        };
        if gap > 0 {
            if let Some(previous) = placed.last_mut() {
                fade_out(&mut previous.samples, fade);
            }
            fade_in(&mut samples, fade);
        }
        heard = true;
        placed.push(PlacedPassage { gap, samples });
    }
    placed
}

pub(crate) fn passage_spans(rate: u32, clips: &[PassageClip]) -> Vec<StorySpan> {
    let rate_f = f64::from(rate.max(1));
    let mut cursor = 0usize;
    place_passages(rate, clips)
        .into_iter()
        .map(|clip| {
            cursor += clip.gap;
            let start = cursor;
            cursor += clip.samples.len();
            StorySpan {
                start: start as f64 / rate_f,
                end: cursor as f64 / rate_f,
            }
        })
        .collect()
}

pub(crate) fn join_passages(rate: u32, clips: &[PassageClip]) -> Vec<i16> {
    let mut audio = Vec::new();
    for clip in place_passages(rate, clips) {
        if clip.gap > 0 {
            audio.resize(audio.len() + clip.gap, 0);
        }
        audio.extend_from_slice(&clip.samples);
    }
    audio
}

pub(crate) fn join_story(rate: u32, pieces: &[StoryPiece]) -> Vec<i16> {
    let planned = plan_story(rate, pieces);
    let total = planned
        .iter()
        .map(|clip| clip.pause + clip.samples.len())
        .sum();
    let mut audio = Vec::with_capacity(total);
    for clip in planned {
        audio.resize(audio.len() + clip.pause, 0);
        audio.extend_from_slice(&clip.samples);
    }
    audio
}

pub(crate) fn resample_linear(samples: &[i16], from: u32, to: u32) -> Vec<i16> {
    if from == 0 || to == 0 || from == to || samples.is_empty() {
        return samples.to_vec();
    }
    let out_len = ((samples.len() as u64) * u64::from(to) / u64::from(from)).max(1) as usize;
    let last = samples.len() - 1;
    let mut out = Vec::with_capacity(out_len);
    for index in 0..out_len {
        let pos = index as f64 * samples.len() as f64 / out_len as f64;
        let left = (pos.floor() as usize).min(last);
        let frac = pos - left as f64;
        let a = f64::from(samples[left]);
        let b = f64::from(samples[(left + 1).min(last)]);
        out.push((a + (b - a) * frac).round() as i16);
    }
    out
}

pub(crate) fn mix_at(canvas: &mut Vec<i16>, rate: u32, start_secs: f64, clip: &[i16]) {
    let offset = (start_secs.max(0.0) * f64::from(rate)).round() as usize;
    if canvas.len() < offset + clip.len() {
        canvas.resize(offset + clip.len(), 0);
    }
    for (index, sample) in clip.iter().enumerate() {
        canvas[offset + index] = canvas[offset + index].saturating_add(*sample);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_then_parse_round_trips() {
        let pcm = Pcm {
            rate: 22050,
            samples: vec![0, 1, -1, i16::MAX, i16::MIN],
        };
        let back = parse_wav(&encode_wav(&pcm)).unwrap();
        assert_eq!(back.rate, 22050);
        assert_eq!(back.samples, pcm.samples);
    }

    #[test]
    fn parse_skips_extra_chunks_like_ffmpeg_list() {
        let pcm = Pcm {
            rate: 16000,
            samples: vec![5, 6, 7],
        };
        let mut bytes = encode_wav(&pcm);
        let tail = bytes.split_off(36);
        bytes.extend_from_slice(b"LIST");
        bytes.extend_from_slice(&4u32.to_le_bytes());
        bytes.extend_from_slice(b"abcd");
        bytes.extend_from_slice(&tail);
        assert_eq!(parse_wav(&bytes).unwrap().samples, vec![5, 6, 7]);
    }

    #[test]
    fn parse_rejects_non_wav_and_stereo() {
        assert!(parse_wav(b"not a wav file at all").is_err());
        let mut bytes = encode_wav(&Pcm {
            rate: 22050,
            samples: vec![1, 2],
        });
        bytes[22] = 2; // channels = 2
        assert!(parse_wav(&bytes).is_err());
    }

    #[test]
    fn mix_extends_canvas_and_saturates() {
        let mut canvas = vec![0i16; 2];
        mix_at(&mut canvas, 10, 0.1, &[i16::MAX, 5]);
        assert_eq!(canvas, vec![0, i16::MAX, 5]);
        mix_at(&mut canvas, 10, 0.1, &[10, 10]);
        assert_eq!(canvas, vec![0, i16::MAX, 15]);
    }

    #[test]
    fn story_join_uses_a_short_breath_instead_of_the_original_gap() {
        let rate = 1000;
        let tone = vec![1000i16; 100];
        let pieces = [
            StoryPiece {
                gap_before: 0.0,
                samples: tone.clone(),
            },
            StoryPiece {
                gap_before: 0.4,
                samples: tone,
            },
        ];
        let spans = story_spans(rate, &pieces);
        let breath = spans[1].start - spans[0].end;
        assert!((breath - STORY_BREATH).abs() < 0.001, "breath {breath}");
        assert!(spans[1].start < 0.3);
        assert_eq!(
            join_story(rate, &pieces).len(),
            (spans[1].end * f64::from(rate)).round() as usize
        );
    }

    #[test]
    fn story_join_uses_a_paragraph_rest_when_the_source_pauses() {
        let rate = 1000;
        let tone = vec![1000i16; 100];
        let spans = story_spans(
            rate,
            &[
                StoryPiece {
                    gap_before: 0.0,
                    samples: tone.clone(),
                },
                StoryPiece {
                    gap_before: 2.5,
                    samples: tone,
                },
            ],
        );
        let rest = spans[1].start - spans[0].end;
        assert!((rest - STORY_REST).abs() < 0.001, "rest {rest}");
        assert!(rest < 1.0);
    }

    #[test]
    fn story_join_trims_padding_and_fades_each_edge() {
        let rate = 1000;
        let mut padded = vec![0i16; 80];
        padded.extend(std::iter::repeat(8000).take(200));
        padded.extend(std::iter::repeat(0).take(80));
        let pieces = [
            StoryPiece {
                gap_before: 0.0,
                samples: padded,
            },
            StoryPiece {
                gap_before: 0.1,
                samples: vec![8000; 200],
            },
        ];
        let spans = story_spans(rate, &pieces);
        let dur = spans[0].end - spans[0].start;
        assert!(dur < 0.25, "duration {dur}");
        let audio = join_story(rate, &pieces);
        let at = (spans[1].start * f64::from(rate)).round() as usize;
        assert!(audio[at].abs() < 4000, "join starts at {}", audio[at]);
        assert!(audio[at + 40].abs() > 7000);
    }

    #[test]
    fn story_join_skips_a_silent_clip_without_an_extra_hole() {
        let rate = 1000;
        let tone = vec![1000i16; 100];
        let spans = story_spans(
            rate,
            &[
                StoryPiece {
                    gap_before: 0.0,
                    samples: tone.clone(),
                },
                StoryPiece {
                    gap_before: 0.2,
                    samples: vec![0; 50],
                },
                StoryPiece {
                    gap_before: 0.1,
                    samples: tone,
                },
            ],
        );
        assert_eq!(spans[1].start, spans[1].end);
        let breath = spans[2].start - spans[0].end;
        assert!((breath - STORY_BREATH).abs() < 0.001, "breath {breath}");
    }

    #[test]
    fn a_cap_split_lets_the_ending_finish() {
        let rate = 1000;
        let clips = [
            PassageClip {
                rest_before: false,
                samples: vec![1000; 1000],
            },
            PassageClip {
                rest_before: false,
                samples: vec![3000; 1000],
            },
        ];
        let tail = secs_to_samples(PASSAGE_TAIL, rate);
        let audio = join_passages(rate, &clips);
        assert_eq!(audio.len(), 2000 + tail);
        let spans = passage_spans(rate, &clips);
        let gap = spans[1].start - spans[0].end;
        assert!((gap - PASSAGE_TAIL).abs() < 0.001, "gap {gap}");
        let at = (spans[0].end * f64::from(rate)).round() as usize;
        assert_eq!(audio[at], 0);
        assert!(audio[at + tail / 2] == 0);
    }

    #[test]
    fn a_source_pause_inserts_the_paragraph_rest() {
        let rate = 1000;
        let clips = [
            PassageClip {
                rest_before: false,
                samples: vec![8000; 1000],
            },
            PassageClip {
                rest_before: true,
                samples: vec![8000; 1000],
            },
        ];
        let spans = passage_spans(rate, &clips);
        let rest = spans[1].start - spans[0].end;
        assert!((rest - STORY_REST).abs() < 0.001, "rest {rest}");
        assert_eq!(join_passages(rate, &clips).len(), 2450);
    }

    #[test]
    fn a_silent_rest_is_kept_on_the_next_audible_clip() {
        let rate = 1000;
        let clips = [
            PassageClip {
                rest_before: false,
                samples: vec![8000; 1000],
            },
            PassageClip {
                rest_before: true,
                samples: vec![0; 400],
            },
            PassageClip {
                rest_before: false,
                samples: vec![8000; 1000],
            },
        ];
        let spans = passage_spans(rate, &clips);
        assert_eq!(spans.len(), 2);
        let rest = spans[1].start - spans[0].end;
        assert!((rest - STORY_REST).abs() < 0.001, "rest {rest}");
    }

    #[test]
    fn a_silent_cap_split_still_lets_the_ending_finish() {
        let rate = 1000;
        let clips = [
            PassageClip {
                rest_before: false,
                samples: vec![1000; 1000],
            },
            PassageClip {
                rest_before: false,
                samples: vec![0; 400],
            },
            PassageClip {
                rest_before: false,
                samples: vec![3000; 1000],
            },
        ];
        let audio = join_passages(rate, &clips);
        let tail = secs_to_samples(PASSAGE_TAIL, rate);
        assert_eq!(audio.len(), 2000 + tail);
        let spans = passage_spans(rate, &clips);
        assert_eq!(spans.len(), 2);
        let gap = spans[1].start - spans[0].end;
        assert!((gap - PASSAGE_TAIL).abs() < 0.001, "gap {gap}");
    }

    #[test]
    fn write_then_read_file() {
        let dir = std::env::temp_dir().join(format!("wav-io-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.wav");
        write_wav(
            &path,
            &Pcm {
                rate: 22050,
                samples: vec![1, 2, 3],
            },
        )
        .unwrap();
        assert_eq!(read_wav(&path).unwrap().samples, vec![1, 2, 3]);
        assert!(!dir.join("a.tmp").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
