use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};

use crate::DubControl;

use crate::job::{
    file_ready, saved_picture, set_source_media, set_source_picture, source_url, write_json, Cue,
    Job, TranslatedLine,
};

use super::media::{
    media_duration, parse_clock, run_tool, run_tool_with, settle_transcript, LineSplitter,
};
use super::{require_tool, stopped};

// Accepts one YouTube, Douyin, or TikTok video link. A pasted string can never become a yt-dlp option or another site.
// Douyin and TikTok share text is the sentence the app copies; the URL inside it is what gets opened.
pub(crate) fn video_url(raw: &str) -> Result<String, String> {
    let url = http_url(raw.trim())
        .ok_or("Dán đủ link YouTube, Douyin hoặc TikTok, bắt đầu bằng https://.")?;
    if url.chars().any(char::is_whitespace) {
        return Err("Link không được có khoảng trắng.".into());
    }
    let host = url_host(url).ok_or("Link không đúng.")?;
    if youtube_host(&host) {
        if raw.trim() == url {
            Ok(url.to_string())
        } else {
            Err("Link YouTube không được có khoảng trắng.".into())
        }
    } else if douyin_video(url, &host) || tiktok_video(url, &host) {
        Ok(url.to_string())
    } else {
        Err("Chỉ nhận link YouTube (youtube.com hoặc youtu.be), Douyin (douyin.com/video hoặc v.douyin.com) hoặc TikTok (tiktok.com/@tên/video, vm.tiktok.com hoặc vt.tiktok.com).".into())
    }
}

fn http_url(raw: &str) -> Option<&str> {
    let start = raw.find("https://").or_else(|| raw.find("http://"))?;
    let rest = &raw[start..];
    let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
    let url = rest[..end].trim_end_matches(|c: char| ".,，。!！)）]」".contains(c));
    (url.len() > "https://".len()).then_some(url)
}

fn url_host(url: &str) -> Option<String> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = host.split('@').last().unwrap_or("");
    let host = host.split(':').next().unwrap_or("").to_ascii_lowercase();
    (!host.is_empty()).then_some(host)
}

fn youtube_host(host: &str) -> bool {
    matches!(
        host,
        "youtube.com" | "www.youtube.com" | "m.youtube.com" | "youtu.be"
    )
}

fn douyin_host(host: &str) -> bool {
    matches!(
        host,
        "douyin.com" | "www.douyin.com" | "m.douyin.com" | "v.douyin.com"
    )
}

fn tiktok_host(host: &str) -> bool {
    matches!(
        host,
        "tiktok.com" | "www.tiktok.com" | "m.tiktok.com" | "vm.tiktok.com" | "vt.tiktok.com"
    )
}

// yt-dlp's Douyin extractor matches /video/<id>. Short links on v.douyin.com are followed by its generic extractor.
fn douyin_video(url: &str, host: &str) -> bool {
    match host {
        "v.douyin.com" => douyin_short_code(url).is_some(),
        "douyin.com" | "www.douyin.com" | "m.douyin.com" => douyin_aweme_id(url).is_some(),
        _ => false,
    }
}

fn douyin_short_code(url: &str) -> Option<String> {
    let code = url_path(url)
        .split('/')
        .find(|part| !part.is_empty())
        .unwrap_or("");
    share_code(code)
}

// vm.tiktok.com and vt.tiktok.com are one code. www.tiktok.com/t/<code> is the other short form.
fn tiktok_video(url: &str, host: &str) -> bool {
    match host {
        "vm.tiktok.com" | "vt.tiktok.com" => tiktok_short_code(url).is_some(),
        "tiktok.com" | "www.tiktok.com" | "m.tiktok.com" => {
            tiktok_item_id(url).is_some() || tiktok_web_short(url).is_some()
        }
        _ => false,
    }
}

fn tiktok_short_code(url: &str) -> Option<String> {
    let mut parts = url_path(url).split('/').filter(|part| !part.is_empty());
    let code = parts.next().unwrap_or("");
    if parts.next().is_some() {
        return None;
    }
    share_code(code)
}

fn tiktok_web_short(url: &str) -> Option<String> {
    let mut parts = url_path(url).split('/').filter(|part| !part.is_empty());
    if parts.next() != Some("t") {
        return None;
    }
    let code = parts.next().unwrap_or("");
    if parts.next().is_some() {
        return None;
    }
    share_code(code)
}

fn tiktok_item_id(url: &str) -> Option<String> {
    let mut parts = url_path(url).split('/').filter(|part| !part.is_empty());
    let user = parts.next()?.strip_prefix('@')?;
    if !tiktok_handle(user) || parts.next() != Some("video") {
        return None;
    }
    let id = parts.next().unwrap_or("");
    if parts.next().is_some() {
        return None;
    }
    aweme_id(id).then(|| id.to_string())
}

fn tiktok_handle(name: &str) -> bool {
    (1..=24).contains(&name.len())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
}

fn share_code(code: &str) -> Option<String> {
    let ok = (1..=32).contains(&code.len())
        && code
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    ok.then(|| code.to_string())
}

fn douyin_aweme_id(url: &str) -> Option<String> {
    if let Some(id) = query_value(url, "modal_id") {
        if aweme_id(&id) {
            return Some(id);
        }
    }
    let mut parts = url_path(url).split('/').filter(|part| !part.is_empty());
    if parts.next() == Some("video") {
        let id = parts.next().unwrap_or("");
        if aweme_id(id) {
            return Some(id.to_string());
        }
    }
    None
}

fn aweme_id(id: &str) -> bool {
    (16..=20).contains(&id.len()) && id.chars().all(|c| c.is_ascii_digit())
}

fn url_path(url: &str) -> &str {
    let rest = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);
    let after_host = rest.split_once('/').map(|(_, path)| path).unwrap_or("");
    after_host.split(['?', '#']).next().unwrap_or("")
}

fn query_value(url: &str, key: &str) -> Option<String> {
    let query = url.split_once('?')?.1.split('#').next().unwrap_or("");
    let prefix = format!("{key}=");
    query
        .split('&')
        .find_map(|pair| pair.strip_prefix(&prefix).map(|value| value.to_string()))
}

// Douyin không đi qua yt-dlp. Edge cấp cookie khách, Python ký request rồi ghi mp4.
fn download_douyin(
    app: &AppHandle,
    url: &str,
    dir: &Path,
    cancel: &AtomicBool,
) -> Result<String, String> {
    let python = crate::program_path("python")
        .ok_or("Chưa thấy Python trong PATH. Tải Douyin cần Python và Microsoft Edge.")?;
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("douyin")
        .join("fetch.py");
    if !script.is_file() {
        return Err("Thiếu phần tải Douyin trong app.".into());
    }
    let mut command = Command::new(python);
    command.arg(&script).arg(url).arg(dir);
    download_report(app, "Đang mở Edge để hỏi Douyin về video.", None);
    let mut lines = LineSplitter::default();
    let stdout = run_tool_with(
        command,
        Duration::from_secs(2 * 60 * 60),
        cancel,
        &mut |_, text| {
            for line in lines.push(text) {
                let Some(update) = ytdlp_progress(&line) else {
                    continue;
                };
                if update.finished {
                    download_report(app, "Đã tải xong video.", None);
                    continue;
                }
                download_report(app, &update.detail("video"), update.fraction());
            }
        },
    )?;
    stdout
        .lines()
        .map(str::trim)
        .rev()
        .find(|line| !line.is_empty())
        .map(PathBuf::from)
        .filter(|path| file_ready(path))
        .map(|path| path.to_string_lossy().into_owned())
        .ok_or("Không ghi được file video Douyin.".into())
}

// Downloads one YouTube or Douyin video into <output_dir>/tai-ve and returns the file path.
pub(crate) fn download(
    app: &AppHandle,
    url: &str,
    output_dir: &Path,
    cancel: &AtomicBool,
) -> Result<String, String> {
    let url = video_url(url)?;
    if !output_dir.is_dir() {
        return Err("Folder kết quả chưa có.".into());
    }
    let dir = output_dir.join("tai-ve");
    // Đã tải link này rồi thì dùng lại file cũ, không hỏi YouTube lại (yt-dlp mất vài giây chỉ để dò thông tin).
    if let Some(path) = video_id(&url).and_then(|id| downloaded_video(&dir, &id)) {
        return Ok(path.to_string_lossy().into_owned());
    }
    require_tool("ffmpeg")?;
    stopped(cancel)?;
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    if url_host(&url).is_some_and(|host| douyin_host(&host)) {
        return download_douyin(app, &url, &dir, cancel);
    }
    if crate::program_path("yt-dlp").is_none() {
        return Err("Chưa thấy yt-dlp trong PATH. Xem mục yt-dlp ở màn chuẩn bị.".into());
    }
    let site = if url_host(&url).is_some_and(|host| tiktok_host(&host)) {
        "TikTok"
    } else {
        "YouTube"
    };
    let mut dashy = site == "YouTube";
    if let Some(id) = video_id(&url) {
        if dashy {
            // File .part cũ là một mạch byte. Cắt về mốc 10 MB rồi ghi .ytdl để lần sau nối, không tải lại từ đầu.
            keep_youtube_progress(&dir, &id);
        }
        if let Some(note) = resume_note(&dir, &id) {
            download_report(app, &note, None);
        }
    }
    download_report(app, &format!("Đang hỏi {site} về video."), None);
    let chosen = video_id(&url)
        .map(|id| locked_format(&dir, &id))
        .unwrap_or_else(|| default_format().to_string());
    let stdout = loop {
        let use_dashy = dashy;
        let result = run_ytdlp_link(site, |fallback| {
            let mut command = yt_command();
            if fallback {
                download_report(app, "Link này bị chặn. Đang hỏi lại YouTube.", None);
            }
            if let Some(args) = youtube_args(site, fallback, use_dashy) {
                command.args(["--extractor-args"]).arg(args);
            }
            command
                .args([
                    "--no-playlist",
                    "--newline",
                    "--progress",
                    "--continue",
                    "--progress-template",
                    "download:DLP %(progress.status)s %(progress.downloaded_bytes)s %(progress.total_bytes)s %(progress.total_bytes_estimate)s %(progress.speed)s %(progress.eta)s",
                    "--windows-filenames",
                    "-f",
                ])
                .arg(&chosen)
                .args([
                    "--merge-output-format",
                    "mp4",
                    "--no-simulate",
                    "--print",
                    "after_move:filepath",
                    "-o",
                    "%(title).120B [%(id)s].%(ext)s",
                    "-P",
                ])
                .arg(&dir);
            command.arg("--").arg(&url);
            let mut lines = LineSplitter::default();
            let mut part = 1u32;
            let mut finished = false;
            run_tool_with(
                command,
                Duration::from_secs(2 * 60 * 60),
                cancel,
                &mut |_, text| {
                    for line in lines.push(text) {
                        let Some(update) = ytdlp_progress(&line) else {
                            continue;
                        };
                        if update.finished {
                            finished = true;
                            let message = if part == 1 {
                                "Đã có hình. Đang tải tiếng."
                            } else {
                                "Đã tải xong, đang ghép hình và tiếng bằng FFmpeg."
                            };
                            download_report(app, message, None);
                            continue;
                        }
                        if finished {
                            part += 1;
                            finished = false;
                        }
                        let label = match part {
                            1 => "hình".to_string(),
                            2 => "tiếng".to_string(),
                            _ => format!("phần {part}"),
                        };
                        download_report(app, &update.detail(&label), update.fraction());
                    }
                },
            )
        });
        if use_dashy && result.as_ref().is_err_and(|error| dashy_rejected(error)) {
            dashy = false;
            download_report(
                app,
                "Cách tải từng đoạn không dùng được. Đang tải theo cách thường.",
                None,
            );
            continue;
        }
        break result;
    }?;
    let printed = stdout
        .lines()
        .map(str::trim)
        .rev()
        .find(|line| !line.is_empty())
        .map(PathBuf::from)
        .filter(|path| file_ready(path));
    let path = printed
        .or_else(|| newest_video(&dir))
        .ok_or("yt-dlp không ghi được file video.")?;
    Ok(path.to_string_lossy().into_owned())
}

const LISTEN_AUDIO_FORMAT: &str = "ba[abr<=80]/139/ba[ext=m4a]/ba";
const SUB_LANGS: &str = "en-orig,vi-orig,en,vi";

// Chỉ audio: lưu link, lấy phụ đề nếu có, không thì tải tiếng. Không tải file hình.
pub(crate) fn prepare_listen(
    app: &AppHandle,
    url: &str,
    output_dir: &Path,
    cancel: &AtomicBool,
) -> Result<String, String> {
    let url = video_url(url)?;
    if !output_dir.is_dir() {
        return Err("Folder kết quả chưa có.".into());
    }
    let host = url_host(&url).unwrap_or_default();
    if douyin_host(&host) || tiktok_host(&host) {
        return Err(
            "Chế độ chỉ audio dùng link YouTube. Douyin và TikTok vẫn cần tải cả video.".into(),
        );
    }
    if !youtube_host(&host) {
        return Err("Chỉ nhận link YouTube khi không tải video.".into());
    }
    stopped(cancel)?;
    download_report(app, "Đang hỏi YouTube về phụ đề.", None);
    let (title, id) = youtube_title(&url, cancel)?;
    let stem = listen_stem(&title, &id);
    let job = Job::open_listen(output_dir, &stem, &url)?;
    crate::note_live_video(app, &job.root);
    if let Some(cues) = settle_transcript(&job) {
        download_report(
            app,
            &format!("Đã có {} câu phụ đề, không tải video.", cues.len()),
            None,
        );
        return Ok(job.root.to_string_lossy().into_owned());
    }
    let captions = job.root.join("captions");
    match fetch_subs(&url, &captions, cancel) {
        Err(error) if error == "Đã dừng." => return Err(error),
        Err(_) => {}
        Ok(()) => {}
    }
    if let Some(path) = pick_subtitle(&captions) {
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        let ext = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        let cues = cues_from_subtitle(&text, ext).unwrap_or_default();
        if !cues.is_empty() {
            write_json(&job.transcript(), &cues)?;
            download_report(
                app,
                &format!("Đã lấy {} câu phụ đề, không tải video.", cues.len()),
                None,
            );
            return Ok(job.root.to_string_lossy().into_owned());
        }
    }
    download_report(app, "Không có phụ đề. Đang tải tiếng.", None);
    require_tool("ffmpeg")?;
    let audio = download_listen_audio(app, &url, &output_dir.join("tai-ve"), &stem, cancel)?;
    set_source_media(&job.root, &audio)?;
    download_report(app, "Đã tải tiếng. Chưa tải video.", None);
    Ok(job.root.to_string_lossy().into_owned())
}

// Tải file hình của link đã lưu, rồi trả folder việc để ghép tiếp.
pub(crate) fn attach_video(
    app: &AppHandle,
    result: &Path,
    output_dir: &Path,
    cancel: &AtomicBool,
) -> Result<String, String> {
    let root = if result.is_dir() {
        result.to_path_buf()
    } else {
        result
            .parent()
            .ok_or("Chưa thấy folder việc.")?
            .to_path_buf()
    };
    if !root.join("source.json").is_file() {
        return Err("Chưa thấy việc đã lưu.".into());
    }
    crate::note_live_video(app, &root);
    if saved_picture(&root).is_some() {
        return Ok(root.to_string_lossy().into_owned());
    }
    let url = source_url(&root);
    if url.is_empty() {
        return Err("Chưa lưu link của video này.".into());
    }
    let path = download(app, &url, output_dir, cancel)?;
    set_source_picture(&root, Path::new(&path))?;
    Ok(root.to_string_lossy().into_owned())
}

// Python trên Windows ghi pipe theo bảng mã ANSI. Chữ trong bảng đó (ê, ô, á) không phải UTF-8,
// chữ ngoài bảng (ậ, ư, ở) thành "?". Tên folder mất dấu tiếng Việt.
fn yt_command() -> Command {
    let mut command = Command::new("yt-dlp");
    command.env("PYTHONUTF8", "1");
    command.env("PYTHONIOENCODING", "utf-8");
    command
}

// Chỉ link YouTube vừa bị chặn mới dùng. Client mặc định giữ cho link tải được.
fn youtube_fallback(command: &mut Command) {
    command.args(["--extractor-args", "youtube:player_client=tv,web_embedded"]);
}

fn browser_wall(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    lower.contains("sign in to confirm")
        || lower.contains("fresh cookies")
        || lower.contains("--cookies")
        || lower.contains("login required")
        || lower.contains("too many requests")
        || lower.contains("http error 429")
}

fn run_ytdlp_link<T, F>(site: &str, mut run: F) -> Result<T, String>
where
    F: FnMut(bool) -> Result<T, String>,
{
    match run(false) {
        Err(error) if site == "YouTube" && browser_wall(&error) => run(true),
        other => other,
    }
    .map_err(|error| download_error(error, site))
}

// Tên để hiện trên hàng đợi. Hỏi oEmbed trước (chỉ chữ, không tải file).
// YouTube và TikTok không trả tên thì hỏi yt-dlp, vẫn không tải.
pub(crate) fn link_title(raw: &str) -> Result<String, String> {
    let url = video_url(raw)?;
    let host = url_host(&url).unwrap_or_default();
    if let Some(title) = oembed_title(&url, &host) {
        return Ok(title);
    }
    let cancel = AtomicBool::new(false);
    if youtube_host(&host) {
        let (title, _) = youtube_title(&url, &cancel)?;
        return one_line_title(&title).ok_or_else(|| "Không đọc được tên video.".to_string());
    }
    if tiktok_host(&host) {
        return printed_title(&url, "TikTok", &cancel);
    }
    Err("Không lấy được tên video.".into())
}

fn oembed_title(url: &str, host: &str) -> Option<String> {
    let endpoint = oembed_endpoint(url, host)?;
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(5))
        .timeout_read(Duration::from_secs(8))
        .build();
    let body = agent
        .get(&endpoint)
        .set(
            "User-Agent",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
        )
        .set("Accept", "application/json")
        .call()
        .ok()?
        .into_string()
        .ok()?;
    title_from_oembed(&body)
}

fn oembed_endpoint(url: &str, host: &str) -> Option<String> {
    let base = if youtube_host(host) {
        "https://www.youtube.com/oembed?format=json&url="
    } else if tiktok_host(host) {
        "https://www.tiktok.com/oembed?url="
    } else {
        return None;
    };
    Some(format!("{base}{}", query_escape(url)))
}

fn title_from_oembed(body: &str) -> Option<String> {
    let value: Value = serde_json::from_str(body).ok()?;
    one_line_title(value.get("title")?.as_str()?)
}

fn one_line_title(raw: &str) -> Option<String> {
    let title = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if title.is_empty() || title.eq_ignore_ascii_case("na") {
        return None;
    }
    Some(truncate_chars(&title, 180))
}

fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    text.chars()
        .take(max)
        .collect::<String>()
        .trim()
        .to_string()
}

fn query_escape(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn printed_title(url: &str, site: &str, cancel: &AtomicBool) -> Result<String, String> {
    let stdout = run_ytdlp_link(site, |fallback| {
        let mut command = ytdlp()?;
        if fallback {
            youtube_fallback(&mut command);
        }
        command.args(["--skip-download", "--print", "%(title)s", "--", url]);
        run_tool(command, Duration::from_secs(45), cancel)
    })?;
    let line = stdout
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("");
    one_line_title(line).ok_or_else(|| "Không đọc được tên video.".to_string())
}

fn youtube_title(url: &str, cancel: &AtomicBool) -> Result<(String, String), String> {
    let stdout = run_ytdlp_link("YouTube", |fallback| {
        let mut command = ytdlp()?;
        if fallback {
            youtube_fallback(&mut command);
        }
        command.args([
            "--skip-download",
            "--print",
            "%(title)s",
            "--print",
            "%(id)s",
            "--",
            url,
        ]);
        run_tool(command, Duration::from_secs(120), cancel)
    })?;
    let mut lines = stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty());
    let title = lines.next().unwrap_or("").to_string();
    let id = lines.next().unwrap_or("").to_string();
    if title.is_empty() || title.eq_ignore_ascii_case("na") {
        return Err("Không đọc được tên video.".into());
    }
    Ok((title, id))
}

fn listen_stem(title: &str, id: &str) -> String {
    let title = truncate_bytes(title.trim(), 120);
    let raw = if id.is_empty() || id.eq_ignore_ascii_case("na") {
        title
    } else {
        format!("{title} [{id}]")
    };
    crate::job::folder_name(Path::new(&raw))
}

fn truncate_bytes(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    let mut end = max;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].trim().to_string()
}

fn fetch_subs(url: &str, dir: &Path, cancel: &AtomicBool) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    run_ytdlp_link("YouTube", |fallback| {
        let mut command = ytdlp()?;
        if fallback {
            youtube_fallback(&mut command);
        }
        command.args([
            "--skip-download",
            "--write-subs",
            "--write-auto-subs",
            "--sub-langs",
            SUB_LANGS,
            "--sub-format",
            "json3/srt/vtt",
            "-o",
            "%(title).120B [%(id)s].%(ext)s",
            "-P",
        ]);
        command.arg(dir);
        command.arg("--").arg(url);
        run_tool(command, Duration::from_secs(180), cancel)?;
        Ok(())
    })?;
    Ok(())
}

fn ytdlp() -> Result<Command, String> {
    if crate::program_path("yt-dlp").is_none() {
        return Err("Chưa thấy yt-dlp trong PATH. Xem mục yt-dlp ở màn chuẩn bị.".into());
    }
    let mut command = yt_command();
    command.args(["--no-playlist", "--newline", "--windows-filenames"]);
    Ok(command)
}

fn pick_subtitle(dir: &Path) -> Option<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .and_then(subtitle_rank)
                .is_some()
        })
        .collect();
    files.sort_by_key(|path| {
        path.file_name()
            .and_then(|name| name.to_str())
            .and_then(subtitle_rank)
            .unwrap_or((9, 9))
    });
    files.into_iter().next()
}

// Bản gốc trước bản thường. Trong cùng loại ngôn ngữ thì json3, rồi srt, rồi vtt.
fn subtitle_rank(name: &str) -> Option<(u8, u8)> {
    let lower = name.to_ascii_lowercase();
    let ext_rank = if lower.ends_with(".json3") {
        0
    } else if lower.ends_with(".srt") {
        1
    } else if lower.ends_with(".vtt") {
        2
    } else {
        return None;
    };
    let stem = lower
        .trim_end_matches(".json3")
        .trim_end_matches(".srt")
        .trim_end_matches(".vtt");
    let lang = stem.rsplit_once('.').map(|(_, lang)| lang).unwrap_or("");
    let lang_rank = if lang.ends_with("-orig") {
        0
    } else if lang == "en" || lang == "vi" {
        1
    } else if lang.is_empty() {
        return None;
    } else {
        2
    };
    Some((lang_rank, ext_rank))
}

fn cues_from_subtitle(text: &str, ext: &str) -> Result<Vec<Cue>, String> {
    let ext = ext.to_ascii_lowercase();
    let cues = if ext == "json3" {
        cues_from_json3(text)?
    } else if ext == "srt" {
        cues_from_timed(text, false)?
    } else if ext == "vtt" {
        cues_from_timed(text, true)?
    } else {
        return Err("Không đọc được phụ đề.".into());
    };
    Ok(collapse_rolling(cues))
}

fn cues_from_json3(text: &str) -> Result<Vec<Cue>, String> {
    let value: Value =
        serde_json::from_str(text).map_err(|_| "Không đọc được phụ đề.".to_string())?;
    let events = value
        .get("events")
        .and_then(Value::as_array)
        .ok_or("Không đọc được phụ đề.")?;
    let mut cues = Vec::new();
    for event in events {
        let start_ms = event.get("tStartMs").and_then(Value::as_f64).unwrap_or(0.0);
        let duration_ms = event.get("dDurationMs").and_then(Value::as_f64);
        let Some(segs) = event.get("segs").and_then(Value::as_array) else {
            continue;
        };
        let mut raw = String::new();
        for seg in segs {
            if let Some(utf8) = seg.get("utf8").and_then(Value::as_str) {
                raw.push_str(utf8);
            }
        }
        let text = clean_caption(&raw);
        if text.is_empty() {
            continue;
        }
        let start = start_ms / 1000.0;
        let end = duration_ms
            .map(|duration| start + duration / 1000.0)
            .unwrap_or(start + 0.05);
        cues.push(Cue {
            start,
            end: end.max(start + 0.05),
            text,
        });
    }
    Ok(cues)
}

fn cues_from_timed(text: &str, vtt: bool) -> Result<Vec<Cue>, String> {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut cues = Vec::new();
    for block in text.split("\n\n") {
        let lines: Vec<&str> = block
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect();
        if lines.is_empty() || lines[0].starts_with("WEBVTT") || lines[0].starts_with("NOTE") {
            continue;
        }
        let time_at = lines.iter().position(|line| line.contains("-->"));
        let Some(time_at) = time_at else {
            continue;
        };
        let Some((start_text, end_text)) = lines[time_at].split_once("-->") else {
            continue;
        };
        let spoken = clean_caption(&lines[time_at + 1..].join(" "));
        if spoken.is_empty() {
            continue;
        }
        let Some(start) = parse_clock(start_text.trim()) else {
            continue;
        };
        let end_text = end_text.trim().split_whitespace().next().unwrap_or("");
        let Some(end) = parse_clock(end_text) else {
            continue;
        };
        let _ = vtt;
        cues.push(Cue {
            start,
            end: end.max(start + 0.05),
            text: spoken,
        });
    }
    Ok(cues)
}

fn clean_caption(text: &str) -> String {
    let text = text
        .replace('\n', " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ");
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let lower = text.to_ascii_lowercase();
    if matches!(
        lower.as_str(),
        "[music]" | "[applause]" | "[laughter]" | "[âm nhạc]" | "♪" | "♫"
    ) {
        return String::new();
    }
    if !text.chars().any(|ch| ch.is_alphanumeric()) {
        return String::new();
    }
    text
}

fn collapse_rolling(cues: Vec<Cue>) -> Vec<Cue> {
    let mut out: Vec<Cue> = Vec::new();
    for cue in cues {
        if let Some(prev) = out.last_mut() {
            if cue.text == prev.text {
                prev.end = cue.end.max(prev.end);
                continue;
            }
            if cue.text.starts_with(prev.text.as_str()) && cue.start <= prev.end + 0.4 {
                prev.text = cue.text;
                prev.end = cue.end.max(prev.end);
                continue;
            }
        }
        out.push(cue);
    }
    out
}

fn download_listen_audio(
    app: &AppHandle,
    url: &str,
    dir: &Path,
    stem: &str,
    cancel: &AtomicBool,
) -> Result<PathBuf, String> {
    if let Some(path) = ready_audio(dir, stem) {
        return Ok(path);
    }
    std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    stopped(cancel)?;
    let stdout = run_ytdlp_link("YouTube", |fallback| {
        let mut command = ytdlp()?;
        if fallback {
            download_report(app, "Link này bị chặn. Đang hỏi lại YouTube.", None);
            youtube_fallback(&mut command);
        }
        command.args([
            "--progress",
            "--progress-template",
            "download:DLP %(progress.status)s %(progress.downloaded_bytes)s %(progress.total_bytes)s %(progress.total_bytes_estimate)s %(progress.speed)s %(progress.eta)s",
            "-f",
            LISTEN_AUDIO_FORMAT,
            "--no-simulate",
            "--print",
            "after_move:filepath",
            "-o",
        ]);
        command.arg(format!("{stem}.%(ext)s"));
        command.arg("-P").arg(dir);
        command.arg("--").arg(url);
        let mut lines = LineSplitter::default();
        run_tool_with(
            command,
            Duration::from_secs(60 * 60),
            cancel,
            &mut |_, text| {
                for line in lines.push(text) {
                    let Some(update) = ytdlp_progress(&line) else {
                        continue;
                    };
                    if update.finished {
                        download_report(app, "Đã tải xong tiếng.", None);
                        continue;
                    }
                    download_report(app, &update.detail("tiếng"), update.fraction());
                }
            },
        )
    })?;
    let printed = stdout
        .lines()
        .map(str::trim)
        .rev()
        .find(|line| !line.is_empty())
        .map(PathBuf::from)
        .filter(|path| file_ready(path));
    printed
        .or_else(|| ready_audio(dir, stem))
        .ok_or("yt-dlp không ghi được file tiếng.".to_string())
}

fn ready_audio(dir: &Path, stem: &str) -> Option<PathBuf> {
    ["m4a", "webm", "opus", "ogg", "mp3", "aac", "m4b"]
        .iter()
        .find_map(|ext| {
            let path = dir.join(format!("{stem}.{ext}"));
            file_ready(&path).then_some(path)
        })
}

pub(super) fn timeline_secs(
    video: &Path,
    lines_end: f64,
    cancel: &AtomicBool,
) -> Result<f64, String> {
    if file_ready(video) {
        if let Ok(secs) = media_duration(video, cancel) {
            return Ok(secs.max(lines_end));
        }
    }
    if lines_end > 0.0 {
        return Ok(lines_end);
    }
    Err("Không đọc được độ dài.".into())
}

pub(super) fn lines_end(lines: &[TranslatedLine]) -> f64 {
    lines.iter().map(|line| line.end).fold(0.0, f64::max)
}

#[derive(Serialize, Clone)]
pub(crate) struct DownloadEvent {
    pub(super) detail: String,
    // Fraction 0..1 of the current file, or None while it cannot be measured.
    pub(super) progress: Option<f64>,
}

fn download_report(app: &AppHandle, detail: &str, progress: Option<f64>) {
    let event = DownloadEvent {
        detail: detail.into(),
        progress,
    };
    app.state::<DubControl>()
        .update(|snap| snap.download = Some(event.clone()));
    let _ = app.emit("download-progress", event);
}

struct YtdlpProgress {
    finished: bool,
    downloaded: f64,
    total: Option<f64>,
    speed: Option<f64>,
    eta: Option<f64>,
}

impl YtdlpProgress {
    fn fraction(&self) -> Option<f64> {
        self.total
            .filter(|total| *total > 0.0)
            .map(|total| (self.downloaded / total).clamp(0.0, 1.0))
    }

    fn detail(&self, label: &str) -> String {
        let mut text = format!("Đang tải {label}");
        if let Some(fraction) = self.fraction() {
            text.push_str(&format!(" · {:.0}%", fraction * 100.0));
        }
        text.push_str(&format!(" · {}", megabytes(self.downloaded)));
        if let Some(total) = self.total {
            text.push_str(&format!(" / {}", megabytes(total)));
        }
        if let Some(speed) = self.speed.filter(|speed| *speed > 0.0) {
            text.push_str(&format!(" · {}/s", megabytes(speed)));
        }
        if let Some(eta) = self.eta {
            text.push_str(&format!(" · còn khoảng {} giây", eta.round() as u64));
        }
        text
    }
}

fn megabytes(bytes: f64) -> String {
    format!("{:.1} MB", bytes / 1_048_576.0)
}

// "DLP downloading 1024 223779 NA 464521.6 0" -> fields; "NA" means yt-dlp does not know the value.
fn ytdlp_progress(line: &str) -> Option<YtdlpProgress> {
    let mut fields = line.trim().strip_prefix("DLP ")?.split_whitespace();
    let status = fields.next()?;
    let mut number = || fields.next().and_then(|value| value.parse::<f64>().ok());
    let downloaded = number()?;
    let total = number();
    let estimate = number();
    let speed = number();
    let eta = number();
    Some(YtdlpProgress {
        finished: status == "finished",
        downloaded,
        total: total.or(estimate),
        speed,
        eta,
    })
}

// Video id used to reuse a file already in tai-ve. Short links have no id until the downloader follows them.
fn video_id(url: &str) -> Option<String> {
    if url_host(url).is_some_and(|host| douyin_host(&host)) {
        return douyin_aweme_id(url);
    }
    if url_host(url).is_some_and(|host| tiktok_host(&host)) {
        return tiktok_item_id(url);
    }
    let rest = url.split_once("://")?.1;
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    let path = path.split(['?', '#']).next().unwrap_or("");
    let candidate = if host.eq_ignore_ascii_case("youtu.be") {
        path.split('/').next().unwrap_or("").to_string()
    } else {
        let query = rest
            .split_once('?')
            .map(|(_, q)| q.split('#').next().unwrap_or(""))
            .unwrap_or("");
        let from_query = query.split('&').find_map(|pair| pair.strip_prefix("v="));
        match from_query {
            Some(id) => id.to_string(),
            None => {
                let mut parts = path.split('/');
                match parts.next() {
                    Some("shorts" | "embed" | "live") => parts.next().unwrap_or("").to_string(),
                    _ => String::new(),
                }
            }
        }
    };
    let valid = candidate.len() == 11
        && candidate
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    valid.then_some(candidate)
}

const YOUTUBE_CHUNK: u64 = 10 * 1024 * 1024;

fn default_format() -> &'static str {
    "bv*[height<=1080]+ba/b[height<=1080]/b"
}

fn youtube_args(site: &str, fallback: bool, dashy: bool) -> Option<String> {
    if site != "YouTube" {
        return None;
    }
    let mut parts = Vec::new();
    if fallback {
        parts.push("player_client=tv,web_embedded");
    }
    if dashy {
        parts.push("formats=dashy");
    }
    if parts.is_empty() {
        None
    } else {
        Some(format!("youtube:{}", parts.join(";")))
    }
}

fn dashy_rejected(error: &str) -> bool {
    if error.contains("Đã dừng.") {
        return false;
    }
    let lower = error.to_ascii_lowercase();
    lower.contains("requested format is not available")
        || lower.contains("no video formats")
        || lower.contains("format not available")
}

// yt-dlp ghi hình và tiếng riêng: `tên [id].f397.mp4` rồi `.f251.webm`. Khóa đúng hai mã đó
// để lần chạy sau không chọn mã khác và tải lại từ đầu.
fn locked_format(dir: &Path, id: &str) -> String {
    let base = default_format();
    let (video, audio) = saved_streams(dir, id);
    match (
        video.as_ref().map(|item| item.format_id.as_str()),
        audio.as_ref().map(|item| item.format_id.as_str()),
    ) {
        (Some(video), Some(audio)) => format!("{video}+{audio}/{base}"),
        (Some(video), None) => format!("{video}+ba/{base}"),
        (None, Some(audio)) => format!("bv*[height<=1080]+{audio}/{base}"),
        (None, None) => base.to_string(),
    }
}

fn resume_note(dir: &Path, id: &str) -> Option<String> {
    let (video, audio) = saved_streams(dir, id);
    match video {
        Some(video) if !video.partial && audio.as_ref().is_some_and(|item| !item.partial) => {
            Some("Hình và tiếng đã có. Đang ghép, không tải lại.".into())
        }
        Some(video) if !video.partial => Some("Hình đã có trong máy. Chỉ tải nốt tiếng.".into()),
        Some(_) => Some("Tải tiếp phần hình đã có, không tải lại từ đầu.".into()),
        None => None,
    }
}

struct SavedStream {
    format_id: String,
    bytes: u64,
    partial: bool,
}

fn saved_streams(dir: &Path, id: &str) -> (Option<SavedStream>, Option<SavedStream>) {
    let mut video = None;
    let mut audio = None;
    let Ok(entries) = std::fs::read_dir(dir) else {
        return (None, None);
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some((format_id, ext, partial)) = stream_piece(&name, id) else {
            continue;
        };
        let bytes = entry.metadata().map(|meta| meta.len()).unwrap_or(0);
        if bytes == 0 {
            continue;
        }
        let stream = SavedStream {
            format_id,
            bytes,
            partial,
        };
        if video_stream(&ext, &stream.format_id) {
            prefer_stream(&mut video, stream);
        } else if audio_stream(&ext, &stream.format_id) {
            prefer_stream(&mut audio, stream);
        }
    }
    (video, audio)
}

fn prefer_stream(best: &mut Option<SavedStream>, next: SavedStream) {
    let replace = match best {
        None => true,
        Some(current) => match (next.partial, current.partial) {
            (false, true) => true,
            (true, false) => false,
            _ => next.bytes > current.bytes,
        },
    };
    if replace {
        *best = Some(next);
    }
}

fn stream_piece(name: &str, id: &str) -> Option<(String, String, bool)> {
    let partial = name.ends_with(".part");
    let bare = name.strip_suffix(".part").unwrap_or(name);
    let marker = format!("[{id}].f");
    let tail = bare.rsplit_once(&marker)?.1;
    let (format_id, ext) = tail.rsplit_once('.')?;
    let ext = ext.to_ascii_lowercase();
    let format_ok = !format_id.is_empty()
        && format_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    let ext_ok = matches!(
        ext.as_str(),
        "mp4" | "mkv" | "mov" | "m4v" | "webm" | "m4a" | "mp3" | "opus" | "aac" | "ogg"
    );
    (format_ok && ext_ok).then(|| (format_id.to_string(), ext, partial))
}

fn audio_stream(ext: &str, format_id: &str) -> bool {
    const AUDIO: &[&str] = &[
        "139", "140", "141", "249", "250", "251", "256", "258", "327", "328", "338",
    ];
    let bare = format_id.split('-').next().unwrap_or(format_id);
    matches!(ext, "m4a" | "mp3" | "opus" | "aac" | "ogg")
        || (ext == "webm" && AUDIO.contains(&bare))
}

fn video_stream(ext: &str, format_id: &str) -> bool {
    matches!(ext, "mp4" | "mkv" | "mov" | "m4v") || (ext == "webm" && !audio_stream(ext, format_id))
}

// YouTube tải từng đoạn 10 MB. File .part bị ngắt giữa chừng thì cắt về đoạn đã đủ và ghi .ytdl,
// để lần sau nối tiếp. Đoạn dở dưới 10 MB thì bỏ, khỏi ghép trùng lên file cũ.
fn keep_youtube_progress(dir: &Path, id: &str) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let parts: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(".part") && stream_piece(name, id).is_some())
        })
        .collect();
    for path in parts {
        if ytdl_for(&path).is_some_and(|ytdl| file_ready(&ytdl)) {
            continue;
        }
        align_part(&path);
    }
}

fn ytdl_for(part: &Path) -> Option<PathBuf> {
    let name = part.file_name()?.to_str()?;
    let stem = name.strip_suffix(".part")?;
    Some(part.with_file_name(format!("{stem}.ytdl")))
}

fn align_part(path: &Path) {
    let len = std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0);
    let aligned = len - (len % YOUTUBE_CHUNK);
    if aligned == 0 {
        let _ = std::fs::remove_file(path);
        return;
    }
    let Ok(file) = std::fs::OpenOptions::new().write(true).open(path) else {
        return;
    };
    if file.set_len(aligned).is_err() {
        return;
    }
    drop(file);
    let Some(ytdl) = ytdl_for(path) else {
        return;
    };
    let index = aligned / YOUTUBE_CHUNK;
    let _ = std::fs::write(
        ytdl,
        format!(r#"{{"downloader":{{"current_fragment":{{"index":{index}}}}}}}"#),
    );
}

fn downloaded_video(dir: &Path, id: &str) -> Option<PathBuf> {
    let suffixes: Vec<String> = ["mp4", "mkv", "mov", "webm", "m4v"]
        .into_iter()
        .map(|ext| format!("[{id}].{ext}"))
        .collect();
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .find(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    suffixes
                        .iter()
                        .any(|suffix| name.ends_with(suffix.as_str()))
                })
                && file_ready(path)
        })
}

fn download_error(error: String, site: &str) -> String {
    if browser_wall(&error) {
        return format!("{site} đòi xác nhận trình duyệt. Đợi vài phút rồi thử lại.");
    }
    if error == "Đã dừng." {
        return error;
    }
    format!("{error} Nếu lỗi lặp lại, chạy yt-dlp -U trong terminal để cập nhật rồi thử lại.")
}

fn newest_video(dir: &Path) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|entry| {
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            [".mp4", ".mkv", ".mov", ".webm", ".avi", ".m4v"]
                .iter()
                .any(|ext| name.ends_with(ext))
        })
        .filter_map(|entry| Some((entry.metadata().ok()?.modified().ok()?, entry.path())))
        .max_by_key(|(time, _)| *time)
        .map(|(_, path)| path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oembed_asks_youtube_and_tiktok_for_a_title() {
        let yt = oembed_endpoint("https://youtu.be/abc123", "youtu.be").unwrap();
        assert!(yt.starts_with("https://www.youtube.com/oembed?format=json&url="));
        assert!(yt.contains("https%3A%2F%2Fyoutu.be%2Fabc123"));
        let tiktok =
            oembed_endpoint("https://www.tiktok.com/@ten/video/1", "www.tiktok.com").unwrap();
        assert!(tiktok.starts_with("https://www.tiktok.com/oembed?url="));
        assert!(oembed_endpoint("https://v.douyin.com/abc/", "v.douyin.com").is_none());
    }

    #[test]
    fn oembed_body_keeps_a_readable_title() {
        assert_eq!(
            title_from_oembed(r#"{"title":"  Tập  một\nđêm  ","author_name":"x"}"#).as_deref(),
            Some("Tập một đêm")
        );
        assert_eq!(title_from_oembed(r#"{"title":"NA"}"#), None);
        assert_eq!(title_from_oembed("không phải json"), None);
    }

    #[test]
    fn video_id_is_read_from_common_youtube_links() {
        let id = Some("dQw4w9WgXcQ".to_string());
        assert_eq!(
            video_id("https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=5s"),
            id
        );
        assert_eq!(
            video_id("https://www.youtube.com/watch?list=x&v=dQw4w9WgXcQ"),
            id
        );
        assert_eq!(video_id("https://youtu.be/dQw4w9WgXcQ?si=abc"), id);
        assert_eq!(video_id("https://www.youtube.com/shorts/dQw4w9WgXcQ"), id);
        assert_eq!(
            video_id("https://www.youtube.com/playlist?list=PL123"),
            None
        );
        assert_eq!(video_id("https://www.youtube.com/watch?v=short"), None);
    }

    #[test]
    fn ytdlp_progress_lines_are_read() {
        let update = ytdlp_progress("DLP downloading 1048576 4194304 NA 2097152.5 2").unwrap();
        assert!(!update.finished);
        assert_eq!(update.fraction(), Some(0.25));
        assert_eq!(
            update.detail("hình"),
            "Đang tải hình · 25% · 1.0 MB / 4.0 MB · 2.0 MB/s · còn khoảng 2 giây"
        );
        let estimate = ytdlp_progress("DLP downloading 100 NA 400 NA NA").unwrap();
        assert_eq!(estimate.fraction(), Some(0.25));
        assert!(ytdlp_progress("DLP finished 5 5 NA 1 NA").unwrap().finished);
        assert!(ytdlp_progress("C:\\video.mp4").is_none());
    }

    #[test]
    fn stopped_download_keeps_the_finished_video_and_chunks() {
        let dir = std::env::temp_dir().join(format!("keep-part-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let id = "abc123defgh";
        let part = dir.join(format!("Talk [{id}].f397.mp4.part"));
        std::fs::write(&part, vec![7u8; YOUTUBE_CHUNK as usize + 50]).unwrap();
        let tiny = dir.join(format!("Talk [{id}].f251.webm.part"));
        std::fs::write(&tiny, vec![1u8; 100]).unwrap();
        let other = dir.join("Khac [zzzzzzzzzzz].f397.mp4.part");
        std::fs::write(&other, vec![3u8; 100]).unwrap();
        keep_youtube_progress(&dir, id);
        assert_eq!(std::fs::metadata(&part).unwrap().len(), YOUTUBE_CHUNK);
        let ytdl = std::fs::read_to_string(dir.join(format!("Talk [{id}].f397.mp4.ytdl"))).unwrap();
        assert!(ytdl.contains("\"index\":1"));
        assert!(!tiny.exists());
        assert_eq!(std::fs::metadata(&other).unwrap().len(), 100);
        assert!(locked_format(&dir, id).starts_with("397+ba/"));
        assert_eq!(
            resume_note(&dir, id).as_deref(),
            Some("Tải tiếp phần hình đã có, không tải lại từ đầu.")
        );
        let picture = dir.join(format!("Talk [{id}].f397.mp4"));
        std::fs::rename(&part, &picture).unwrap();
        let audio = dir.join(format!("Talk [{id}].f251.webm.part"));
        std::fs::write(&audio, vec![2u8; YOUTUBE_CHUNK as usize + 20]).unwrap();
        keep_youtube_progress(&dir, id);
        assert_eq!(std::fs::metadata(&picture).unwrap().len(), YOUTUBE_CHUNK);
        assert_eq!(std::fs::metadata(&audio).unwrap().len(), YOUTUBE_CHUNK);
        assert!(locked_format(&dir, id).starts_with("397+251/"));
        assert_eq!(
            resume_note(&dir, id).as_deref(),
            Some("Hình đã có trong máy. Chỉ tải nốt tiếng.")
        );
        let merged = dir.join(format!("Talk [{id}].mp4"));
        std::fs::write(&merged, b"full").unwrap();
        assert_eq!(
            downloaded_video(&dir, id)
                .unwrap()
                .file_name()
                .unwrap()
                .to_string_lossy(),
            format!("Talk [{id}].mp4")
        );
        let tracked = dir.join(format!("Talk [{id}].f140.m4a.part"));
        std::fs::write(&tracked, vec![9u8; YOUTUBE_CHUNK as usize + 80]).unwrap();
        std::fs::write(
            dir.join(format!("Talk [{id}].f140.m4a.ytdl")),
            b"{\"downloader\":{}}",
        )
        .unwrap();
        keep_youtube_progress(&dir, id);
        assert_eq!(
            std::fs::metadata(&tracked).unwrap().len(),
            YOUTUBE_CHUNK + 80
        );
        assert!(!dashy_rejected("Đã dừng."));
        assert!(dashy_rejected(
            "ERROR: Requested format is not available. Nếu lỗi lặp lại"
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn video_link_is_checked_before_it_reaches_ytdlp() {
        for ok in [
            "https://www.youtube.com/watch?v=abc123",
            " https://youtu.be/abc123?t=5 ",
            "https://m.youtube.com/shorts/abc123",
            "https://www.douyin.com/video/6961737553342991651",
            "https://v.douyin.com/iRNBho6/",
            "https://www.douyin.com/jingxuan?modal_id=6961737553342991651",
            "https://www.tiktok.com/@ten.nguoi/video/7234567890123456789",
            "https://m.tiktok.com/@ten/video/7234567890123456789?is_from_webapp=1",
            "https://vm.tiktok.com/ZMabc123/",
            "https://vt.tiktok.com/ZS123abc/",
            "https://www.tiktok.com/t/ZTxyz789/",
        ] {
            assert!(video_url(ok).is_ok(), "{ok}");
        }
        assert_eq!(
            video_url(" https://youtu.be/abc ").unwrap(),
            "https://youtu.be/abc"
        );
        let share = "7.43 复制打开抖音，看看【作品】标题 https://v.douyin.com/iRNBho6/ 复制此链接";
        assert_eq!(video_url(share).unwrap(), "https://v.douyin.com/iRNBho6/");
        assert_eq!(
            video_id("https://www.douyin.com/video/6961737553342991651"),
            Some("6961737553342991651".into())
        );
        assert_eq!(video_id("https://v.douyin.com/iRNBho6/"), None);
        let tiktok_share = "Xem video này https://vt.tiktok.com/ZS123abc/ trên TikTok";
        assert_eq!(
            video_url(tiktok_share).unwrap(),
            "https://vt.tiktok.com/ZS123abc/"
        );
        assert_eq!(
            video_id("https://www.tiktok.com/@ten/video/7234567890123456789?is_from_webapp=1"),
            Some("7234567890123456789".into())
        );
        assert_eq!(video_id("https://vm.tiktok.com/ZMabc123/"), None);
        for bad in [
            "--exec calc",
            "youtube.com/watch?v=abc",
            "https://evil.com/youtube.com",
            "https://youtube.com.evil.com/watch",
            "https://youtube.com@evil.com/x",
            "https://youtu.be/a b",
            "file:///C:/video.mp4",
            "https://www.douyin.com/",
            "https://www.douyin.com/user/abc",
            "https://v.douyin.com/",
            "https://douyin.com.evil.com/video/6961737553342991651",
            "https://www.tiktok.com/@ten",
            "https://www.tiktok.com/@ten/photo/7234567890123456789",
            "https://www.tiktok.com/@ten/video/123",
            "https://vm.tiktok.com/",
            "https://tiktok.com.evil.com/@ten/video/7234567890123456789",
            "https://www.bilibili.com/video/BV1xx411c7mD",
        ] {
            assert!(video_url(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn subtitle_rank_prefers_original_json3() {
        let names = [
            "Talk [abc].en.srt",
            "Talk [abc].en-orig.vtt",
            "Talk [abc].vi.srt",
            "Talk [abc].en-orig.json3",
        ];
        let best = names
            .iter()
            .copied()
            .min_by_key(|name| subtitle_rank(name).unwrap())
            .unwrap();
        assert_eq!(best, "Talk [abc].en-orig.json3");
        assert!(
            subtitle_rank("Talk [abc].vi-orig.srt").unwrap()
                < subtitle_rank("Talk [abc].vi.srt").unwrap()
        );
    }

    #[test]
    fn youtube_json3_becomes_cues_and_drops_music() {
        let text = r#"{"events":[
            {"tStartMs":0,"dDurationMs":1000,"segs":[{"utf8":"Hello"}]},
            {"tStartMs":200,"dDurationMs":1200,"segs":[{"utf8":"Hello there"}]},
            {"tStartMs":2000,"dDurationMs":800,"segs":[{"utf8":"[Music]"}]},
            {"tStartMs":3000,"dDurationMs":1000,"segs":[{"utf8":"Next line"}]}
        ]}"#;
        let cues = cues_from_subtitle(text, "json3").unwrap();
        assert_eq!(cues.len(), 2);
        assert_eq!(cues[0].text, "Hello there");
        assert_eq!(cues[1].text, "Next line");
        assert!((cues[1].start - 3.0).abs() < 0.001);
    }

    #[test]
    fn srt_and_vtt_cues_skip_blank_and_music() {
        let srt = "1\n00:00:01,000 --> 00:00:02,500\nXin chào\n\n2\n00:00:03,000 --> 00:00:04,000\n[Music]\n";
        let cues = cues_from_subtitle(srt, "srt").unwrap();
        assert_eq!(cues.len(), 1);
        assert_eq!(cues[0].text, "Xin chào");
        assert!((cues[0].start - 1.0).abs() < 0.001);
        let vtt = "WEBVTT\n\n00:00:01.000 --> 00:00:02.000\nHello &amp; there\n";
        let cues = cues_from_subtitle(vtt, "vtt").unwrap();
        assert_eq!(cues.len(), 1);
        assert_eq!(cues[0].text, "Hello & there");
    }

    #[test]
    fn listen_stem_matches_the_saved_folder_name() {
        assert_eq!(listen_stem("Talk: one?", "abc"), "Talk one [abc]");
        assert_eq!(
            listen_stem(
                "Tập 2 | WUWA: Xuyên Không Ta Sở Hữu Thập Chủng Ảnh Pháp",
                "tSe01jpxlRE"
            ),
            "Tập 2  WUWA Xuyên Không Ta Sở Hữu Thập Chủng Ảnh Pháp [tSe01jpxlRE]"
        );
        assert_eq!(LISTEN_AUDIO_FORMAT, "ba[abr<=80]/139/ba[ext=m4a]/ba");
    }
}
