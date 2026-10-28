use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(crate) struct RenderSettings {
    pub logo: crate::media_assets::Logo,
    pub mask: crate::media_assets::SourceMask,
    pub audio: crate::audio_mix::AudioMix,
    pub pronunciations: std::collections::BTreeMap<String, Vec<crate::names::NameFix>>,
    pub aspect: String,
    pub framing: String,
    pub focus_x: f64,
    pub focus_y: f64,
    pub font: String,
    pub size: f64,
    pub color: String,
    pub outline_color: String,
    pub outline: f64,
    pub background: bool,
    pub caption_x: f64,
    pub caption_y: f64,
}

impl Default for RenderSettings {
    fn default() -> Self {
        Self {
            logo: Default::default(),
            mask: Default::default(),
            audio: Default::default(),
            pronunciations: Default::default(),
            aspect: "original".into(),
            framing: "blur".into(),
            focus_x: 50.0,
            focus_y: 50.0,
            font: "Arial".into(),
            size: 4.2,
            color: "#ffffff".into(),
            outline_color: "#101010".into(),
            outline: 1.0,
            background: false,
            caption_x: 50.0,
            caption_y: 88.0,
        }
    }
}

impl RenderSettings {
    pub fn validate(&self) -> Result<(), String> {
        self.audio.validate()?;
        self.logo.validate()?;
        self.mask.validate()?;
        for (lang, rules) in &self.pronunciations {
            if lang.len() != 2
                || !lang.bytes().all(|c| c.is_ascii_lowercase())
                || rules.len() > 200
                || rules.iter().any(|r| {
                    r.from.len() > 300
                        || r.to.len() > 600
                        || r.from.trim().is_empty()
                        || r.to.trim().is_empty()
                })
            {
                return Err("Bảng cách đọc không hợp lệ (tối đa 200 mục mỗi ngôn ngữ).".into());
            }
        }
        if !["original", "9:16", "1:1", "16:9"].contains(&self.aspect.as_str())
            || !["crop", "blur"].contains(&self.framing.as_str())
            || !["Arial", "Tahoma", "Verdana", "Times New Roman"].contains(&self.font.as_str())
        {
            return Err("Lựa chọn khung hình hoặc font không hợp lệ.".into());
        }
        for (v, min, max) in [
            (self.focus_x, 0.0, 100.0),
            (self.focus_y, 0.0, 100.0),
            (self.caption_x, 15.0, 85.0),
            (self.caption_y, 20.0, 92.0),
            (self.size, 2.0, 8.0),
            (self.outline, 0.0, 5.0),
        ] {
            if !v.is_finite() || !(min..=max).contains(&v) {
                return Err("Thông số hình hoặc chữ ngoài phạm vi.".into());
            }
        }
        for color in [&self.color, &self.outline_color] {
            if color.len() != 7
                || !color.starts_with('#')
                || !color[1..].bytes().all(|c| c.is_ascii_hexdigit())
            {
                return Err("Màu chữ không hợp lệ.".into());
            }
        }
        Ok(())
    }

    pub fn read(root: &Path) -> Result<Self, String> {
        let path = root.join("render.json");
        if !path.exists() {
            return Ok(Self::default());
        }
        let settings: Self = crate::job::read_json(&path)
            .map_err(|e| format!("Không đọc được bố cục đã lưu trong render.json: {e}"))?;
        settings.validate()?;
        Ok(settings)
    }

    pub fn dimensions(&self, w: u32, h: u32) -> (u32, u32) {
        let short = w.min(h).min(1080).max(2);
        let even = |v: f64| ((v / 2.0).round() as u32).max(1) * 2;
        match self.aspect.as_str() {
            "9:16" => (even(short as f64), even(short as f64 * 16.0 / 9.0)),
            "16:9" => (even(short as f64 * 16.0 / 9.0), even(short as f64)),
            "1:1" => (even(short as f64), even(short as f64)),
            _ => (w, h),
        }
    }

    pub(crate) fn same_frame(&self, other: &Self) -> bool {
        self.logo.same(&other.logo)
            && ((!self.mask.enabled && !other.mask.enabled) || self.mask == other.mask)
            && self.aspect == other.aspect
            && (self.aspect == "original"
                || (self.framing == other.framing
                    && (self.framing == "blur"
                        || (self.focus_x == other.focus_x && self.focus_y == other.focus_y))))
    }

    pub fn matches_output(
        &self,
        other: &Self,
        audio_only: bool,
        text_only: bool,
        lang: &str,
    ) -> bool {
        if audio_only {
            return crate::names::name_signature(
                self.pronunciations
                    .get(lang)
                    .map(Vec::as_slice)
                    .unwrap_or(&[]),
            ) == crate::names::name_signature(
                other
                    .pronunciations
                    .get(lang)
                    .map(Vec::as_slice)
                    .unwrap_or(&[]),
            );
        }
        if text_only {
            let mut a = self.clone();
            let mut b = other.clone();
            a.audio = Default::default();
            b.audio = Default::default();
            a.pronunciations.clear();
            b.pronunciations.clear();
            return a.equivalent(&b);
        }
        self.equivalent(other)
    }

    pub fn equivalent(&self, other: &Self) -> bool {
        let mut a = self.clone();
        let mut b = other.clone();
        a.logo.path.clear();
        b.logo.path.clear();
        a == b
    }

    pub fn needs_frame(&self) -> bool {
        self.aspect != "original" || self.logo.enabled || self.mask.enabled
    }

    #[cfg(test)]
    pub fn graph(&self, w: u32, h: u32, tail: &str) -> String {
        self.graph_with_logo(w, h, tail, None)
    }

    // The source region is processed before framing; the logo is placed afterwards.
    pub fn graph_with_logo(&self, w: u32, h: u32, tail: &str, logo_input: Option<usize>) -> String {
        let (out_w, out_h) = self.dimensions(w, h);
        let mut graph =
            String::from("[0:v:0]scale=w='trunc(iw*sar/2)*2':h=ih,setsar=1[normalized];");
        let input = if self.mask.enabled {
            let m = &self.mask;
            let (x, y, mw, mh) = (m.x / 100.0, m.y / 100.0, m.width / 100.0, m.height / 100.0);
            if m.mode == "solid" {
                graph.push_str(&format!("[normalized]drawbox=x=iw*{x:.6}:y=ih*{y:.6}:w=iw*{mw:.6}:h=ih*{mh:.6}:color=0x{}:t=fill[masked];",&m.color[1..]));
            } else {
                graph.push_str(&format!("[normalized]split=2[clean][region];[region]crop=w='max(2,trunc(iw*{mw:.6}/2)*2)':h='max(2,trunc(ih*{mh:.6}/2)*2)':x='trunc(iw*{x:.6}/2)*2':y='trunc(ih*{y:.6}/2)*2',boxblur=luma_radius='min(w,h)/8':luma_power=2[blurred];[clean][blurred]overlay=x=W*{x:.6}:y=H*{y:.6}[masked];"));
            }
            "masked"
        } else {
            "normalized"
        };
        if self.aspect == "original" {
            graph.push_str(&format!("[{input}]null[framed];"));
        } else if self.framing == "crop" {
            graph.push_str(&format!("[{input}]scale={out_w}:{out_h}:force_original_aspect_ratio=increase,crop={out_w}:{out_h}:(iw-ow)*{:.6}:(ih-oh)*{:.6},setsar=1[framed];",self.focus_x/100.0,self.focus_y/100.0));
        } else {
            graph.push_str(&format!("[{input}]split=2[bg][fg];[bg]scale={out_w}:{out_h}:force_original_aspect_ratio=increase,crop={out_w}:{out_h},boxblur=20:2[back];[fg]scale={out_w}:{out_h}:force_original_aspect_ratio=decrease[front];[back][front]overlay=(W-w)/2:(H-h)/2,setsar=1[framed];"));
        }
        let last = if self.logo.enabled {
            if let Some(index) = logo_input {
                graph.push_str(&format!("[{index}:v:0]format=rgba,colorchannelmixer=aa={:.6},scale=w={}:h={out_h}:force_original_aspect_ratio=decrease[brand];[framed][brand]overlay=x='(W-w)*{:.6}':y='(H-h)*{:.6}':eof_action=repeat:repeatlast=1[branded];",self.logo.opacity/100.0,(out_w as f64*self.logo.size/100.0).round().max(2.0) as u32,self.logo.x/100.0,self.logo.y/100.0));
                "branded"
            } else {
                "framed"
            }
        } else {
            "framed"
        };
        graph.push_str(&format!("[{last}]null{tail}[vout]"));
        graph
    }
}

// Existing per-line speech is invalidated only if its pronunciation changed.
#[cfg(test)]
pub(crate) fn settle(job: &crate::job::Job, now: &RenderSettings) -> Result<(), String> {
    settle_for(job, now, false, false)
}

pub(crate) fn spoken_text(settings: &RenderSettings, lang: &str, text: &str) -> String {
    crate::names::apply_names(
        text,
        settings
            .pronunciations
            .get(lang)
            .map(Vec::as_slice)
            .unwrap_or(&[]),
    )
}

pub(crate) fn settle_for(
    job: &crate::job::Job,
    now: &RenderSettings,
    audio_only: bool,
    text_only: bool,
) -> Result<(), String> {
    now.validate()?;
    let before = RenderSettings::read(&job.root)?;
    let lines = if job.root.join("editing.json").exists() {
        crate::timing::read(&job.root)?
    } else {
        // Legacy jobs may have an incomplete translation cache while framing
        // is changed. Preserve that tolerant path; edited jobs require mapping.
        crate::job::read_json::<Vec<crate::job::TranslatedLine>>(&job.translation_path()).unwrap_or_default()
    };
    let changed: Vec<_> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| {
            spoken_text(&before, &job.target, &l.translation)
                != spoken_text(now, &job.target, &l.translation)
        })
        .map(|(i, _)| i + 1)
        .collect();
    let speech_changed = !text_only && !changed.is_empty();
    let audio_changed = !audio_only && !text_only && before.audio != now.audio;
    let frame_changed = !audio_only && !before.same_frame(now);
    if speech_changed || audio_changed || frame_changed {
        let saved = crate::job::archive_exports_detail(&job.root)?;
        if audio_changed && !speech_changed && !frame_changed {
            if let Some(path) = saved
                .iter()
                .map(|s| s.join(job.result().file_name().unwrap()))
                .find(|p| p.is_file())
            {
                crate::job::write_json(
                    &job.root.join("frame-cache.json"),
                    &serde_json::json!({"path":path,"render":before}),
                )?;
            }
        } else {
            let _ = std::fs::remove_file(job.root.join("frame-cache.json"));
        }
        if speech_changed {
            for index in changed {
                let _ = std::fs::remove_file(job.line_wav(index));
            }
            for name in ["dub.wav", "fit-report.json", "voice-spans.json"] {
                let _ = std::fs::remove_file(job.root.join(name));
            }
        }
    } else if !audio_only && before != *now {
        crate::job::archive_captioned(job)?;
    }
    crate::job::write_json(&job.root.join("render.json"), now)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn legacy_settings_and_untrusted_input() {
        let legacy: RenderSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(legacy, RenderSettings::default());
        let mut invalid = legacy.clone();
        invalid.font = "Arial,\\pos(0,0)".into();
        assert!(invalid.validate().is_err());
        invalid = legacy;
        invalid.color = "#xyzxyz".into();
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn changing_layout_keeps_work_and_archives_exports() {
        let root = std::env::temp_dir().join(format!("render-cache-{}", std::process::id()));
        std::fs::create_dir_all(root.join("lines")).unwrap();
        let source = root.join("source.mp4");
        let job = crate::job::Job {
            root: root.clone(),
            video: source.clone(),
            picture: source,
            target: "vi".into(),
        };
        for path in [
            job.transcript(),
            job.translation_path(),
            job.line_wav(1),
            job.result(),
            job.captioned(),
        ] {
            std::fs::write(path, b"keep").unwrap();
        }
        let settings = RenderSettings {
            aspect: "9:16".into(),
            ..Default::default()
        };
        settle(&job, &settings).unwrap();
        assert!(
            job.transcript().is_file()
                && job.translation_path().is_file()
                && job.line_wav(1).is_file()
        );
        assert!(!job.result().exists() && !job.captioned().exists());
        assert!(!crate::job::list_kept(&root).unwrap().is_empty());
        assert_eq!(RenderSettings::read(&root).unwrap(), settings);
        std::fs::write(job.result(), b"new export").unwrap();
        settle(&job, &settings).unwrap();
        assert!(job.result().exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn changing_only_text_keeps_plain_video_and_archives_old_overlay() {
        let root = std::env::temp_dir().join(format!("render-style-cache-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let source = root.join("source.mp4");
        let job = crate::job::Job {
            root: root.clone(),
            video: source.clone(),
            picture: source,
            target: "vi".into(),
        };
        let before = RenderSettings {
            aspect: "9:16".into(),
            ..Default::default()
        };
        crate::job::write_json(&root.join("render.json"), &before).unwrap();
        std::fs::write(job.result(), b"plain video").unwrap();
        std::fs::write(job.captioned(), b"old overlay").unwrap();
        std::fs::write(job.subtitles(), b"saved subtitles").unwrap();
        let modified = std::fs::metadata(job.result()).unwrap().modified().unwrap();
        let now = RenderSettings {
            color: "#ffcc00".into(),
            caption_y: 75.0,
            ..before.clone()
        };
        settle(&job, &now).unwrap();
        assert_eq!(std::fs::read(job.result()).unwrap(), b"plain video");
        assert_eq!(
            std::fs::metadata(job.result()).unwrap().modified().unwrap(),
            modified
        );
        assert!(!job.captioned().exists());
        assert!(job.subtitles().exists());
        let kept = crate::job::list_kept(&root).unwrap();
        assert_eq!(kept.len(), 1);
        let old = std::path::PathBuf::from(&kept[0].path);
        assert_eq!(std::fs::read(&old).unwrap(), b"old overlay");
        assert_eq!(RenderSettings::read(old.parent().unwrap()).unwrap(), before);
        assert_eq!(RenderSettings::read(&root).unwrap(), now);
        assert!(before.same_frame(&RenderSettings {
            focus_x: 20.0,
            ..before.clone()
        }));
        assert!(!before.same_frame(&RenderSettings {
            framing: "crop".into(),
            ..before.clone()
        }));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn pronunciation_invalidates_only_affected_speech_and_audio_cache_matches_language() {
        let root = std::env::temp_dir().join(format!("pronunciation-cache-{}", std::process::id()));
        std::fs::create_dir_all(root.join("lines")).unwrap();
        let job = crate::job::Job {
            root: root.clone(),
            video: root.join("input.mp4"),
            picture: root.join("input.mp4"),
            target: "vi".into(),
        };
        let lines = vec![
            crate::job::TranslatedLine {
                start: 0.0,
                end: 2.0,
                source: "London".into(),
                translation: "Tôi ở London.".into(),
            },
            crate::job::TranslatedLine {
                start: 2.0,
                end: 4.0,
                source: "Canada".into(),
                translation: "Canada đẹp.".into(),
            },
        ];
        crate::job::write_json(&job.translation_path(), &lines).unwrap();
        std::fs::write(job.line_wav(1), b"old voice one").unwrap();
        std::fs::write(job.line_wav(2), b"keep voice two").unwrap();
        let before = RenderSettings::default();
        let mut now = before.clone();
        now.pronunciations.insert(
            "vi".into(),
            vec![crate::names::NameFix {
                from: "London".into(),
                to: "Lân Đân".into(),
            }],
        );
        assert!(!before.matches_output(&now, true, false, "vi"));
        assert!(before.matches_output(&now, true, false, "en"));
        assert_eq!(
            spoken_text(&now, "vi", "London và Londonderry"),
            "Lân Đân và Londonderry"
        );
        settle_for(&job, &now, true, false).unwrap();
        assert!(!job.line_wav(1).exists());
        assert_eq!(std::fs::read(job.line_wav(2)).unwrap(), b"keep voice two");
        let saved: Vec<crate::job::TranslatedLine> =
            crate::job::read_json(&job.translation_path()).unwrap();
        assert_eq!(saved, lines);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn ffmpeg_frames_preserve_audio_and_expected_dimensions() {
        let root = std::env::temp_dir().join(format!("render-frames-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        for aspect in ["9:16", "1:1", "16:9"] {
            for framing in ["crop", "blur"] {
                let settings = RenderSettings {
                    aspect: aspect.into(),
                    framing: framing.into(),
                    focus_x: 80.0,
                    ..Default::default()
                };
                let out = root.join(format!("{}-{framing}.mp4", aspect.replace(':', "-")));
                let result = Command::new("ffmpeg")
                    .args([
                        "-v",
                        "error",
                        "-y",
                        "-f",
                        "lavfi",
                        "-i",
                        "testsrc2=s=320x180:r=10:d=0.4",
                        "-f",
                        "lavfi",
                        "-i",
                        "sine=frequency=440:duration=0.4",
                        "-filter_complex",
                        &settings.graph(320, 180, ""),
                        "-map",
                        "[vout]",
                        "-map",
                        "1:a:0",
                        "-c:v",
                        "libx264",
                        "-preset",
                        "ultrafast",
                        "-pix_fmt",
                        "yuv420p",
                        "-c:a",
                        "aac",
                    ])
                    .arg(&out)
                    .output()
                    .expect("FFmpeg must be installed for media checks");
                assert!(
                    result.status.success(),
                    "{}",
                    String::from_utf8_lossy(&result.stderr)
                );
                let probe = Command::new("ffprobe")
                    .args(["-v", "error", "-show_streams", "-of", "json"])
                    .arg(&out)
                    .output()
                    .unwrap();
                let data: serde_json::Value = serde_json::from_slice(&probe.stdout).unwrap();
                let streams = data["streams"].as_array().unwrap();
                let video = streams.iter().find(|s| s["codec_type"] == "video").unwrap();
                let (w, h) = settings.dimensions(320, 180);
                assert_eq!(
                    (
                        video["width"].as_u64().unwrap(),
                        video["height"].as_u64().unwrap()
                    ),
                    (w as u64, h as u64)
                );
                assert_eq!(video["sample_aspect_ratio"], "1:1");
                assert!(streams.iter().any(|s| s["codec_type"] == "audio"));
            }
        }
        let _ = std::fs::remove_dir_all(root);
    }
}
