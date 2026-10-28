use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tauri::Manager;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(crate) struct Logo {
    pub enabled: bool,
    pub path: String,
    pub sha256: String,
    pub size: f64,
    pub opacity: f64,
    pub x: f64,
    pub y: f64,
}
impl Default for Logo {
    fn default() -> Self {
        Self {
            enabled: false,
            path: String::new(),
            sha256: String::new(),
            size: 18.0,
            opacity: 80.0,
            x: 95.0,
            y: 5.0,
        }
    }
}
impl Logo {
    pub fn same(&self, other: &Self) -> bool {
        if !self.enabled && !other.enabled {
            return true;
        }
        let mut a = self.clone();
        let mut b = other.clone();
        a.path.clear();
        b.path.clear();
        a == b
    }
    pub fn validate(&self) -> Result<(), String> {
        if [
            (self.size, 3.0, 50.0),
            (self.opacity, 0.0, 100.0),
            (self.x, 0.0, 100.0),
            (self.y, 0.0, 100.0),
        ]
        .iter()
        .any(|(v, a, b)| !v.is_finite() || v < a || v > b)
            || (self.enabled
                && (self.path.is_empty()
                    || self.sha256.len() != 64
                    || !self.sha256.bytes().all(|c| c.is_ascii_hexdigit())))
        {
            return Err("Logo hoặc vị trí logo không hợp lệ.".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(crate) struct SourceMask {
    pub enabled: bool,
    pub mode: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub color: String,
}
impl Default for SourceMask {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: "blur".into(),
            x: 5.0,
            y: 75.0,
            width: 90.0,
            height: 18.0,
            color: "#000000".into(),
        }
    }
}
impl SourceMask {
    pub fn validate(&self) -> Result<(), String> {
        if !["blur", "solid"].contains(&self.mode.as_str())
            || [self.x, self.y, self.width, self.height]
                .iter()
                .any(|v| !v.is_finite())
            || self.x < 0.0
            || self.y < 0.0
            || self.width < 1.0
            || self.height < 1.0
            || self.x + self.width > 100.001
            || self.y + self.height > 100.001
            || self.color.len() != 7
            || !self.color.starts_with('#')
            || !self.color[1..].bytes().all(|c| c.is_ascii_hexdigit())
        {
            return Err("Vùng che chữ phải nằm trong hình nguồn.".into());
        }
        Ok(())
    }
}

fn read_image(path: &Path) -> Result<(Vec<u8>, &'static str, String), String> {
    let meta = std::fs::metadata(path)
        .map_err(|_| "Không thấy ảnh logo. Chọn lại ảnh trên máy.".to_string())?;
    if !meta.is_file() || meta.len() > 20 * 1024 * 1024 {
        return Err("Chọn ảnh PNG/JPEG nhỏ hơn 20 MB.".into());
    }
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let ext = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        "png"
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        "jpg"
    } else {
        return Err("Logo phải là ảnh PNG hoặc JPEG.".into());
    };
    let hash = format!("{:x}", Sha256::digest(&bytes));
    Ok((bytes, ext, hash))
}

fn save_image(dir: &Path, bytes: &[u8], ext: &str, hash: &str) -> Result<PathBuf, String> {
    use std::io::Write;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{hash}.{ext}"));
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
    {
        Ok(mut file) => {
            if let Err(e) = file.write_all(bytes) {
                let _ = std::fs::remove_file(&path);
                return Err(e.to_string());
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            if read_image(&path)?.2 != hash {
                return Err("Ảnh logo đã lưu bị thay đổi. Chọn lại ảnh.".into());
            }
        }
        Err(e) => return Err(e.to_string()),
    }
    Ok(path)
}

#[tauri::command]
pub(crate) async fn import_logo(app: tauri::AppHandle, path: String) -> Result<Logo, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let (bytes, ext, hash) = read_image(Path::new(&path))?;
        let dir = app
            .path()
            .app_data_dir()
            .map_err(|e| e.to_string())?
            .join("overlays");
        let saved = save_image(&dir, &bytes, ext, &hash)?;
        app.asset_protocol_scope()
            .allow_file(&saved)
            .map_err(|e| e.to_string())?;
        Ok(Logo {
            enabled: true,
            path: saved.to_string_lossy().into_owned(),
            sha256: hash,
            ..Default::default()
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub(crate) fn allow_logo_preview(
    app: tauri::AppHandle,
    path: String,
    sha256: String,
) -> Result<(), String> {
    if read_image(Path::new(&path))?.2 != sha256 {
        return Err("Ảnh logo không khớp bản đã lưu.".into());
    }
    app.asset_protocol_scope()
        .allow_file(path)
        .map_err(|e| e.to_string())
}

pub(crate) fn stage(
    job: &crate::job::Job,
    settings: &mut crate::render::RenderSettings,
) -> Result<(), String> {
    settings.validate()?;
    if !settings.logo.enabled {
        return Ok(());
    }
    let dir = job.root.join("overlays");
    let existing = [
        dir.join(format!("{}.png", settings.logo.sha256)),
        dir.join(format!("{}.jpg", settings.logo.sha256)),
    ]
    .into_iter()
    .find(|p| p.is_file());
    let path = existing.unwrap_or_else(|| PathBuf::from(&settings.logo.path));
    let (bytes, ext, hash) = read_image(&path)?;
    if hash != settings.logo.sha256 {
        return Err("Ảnh logo đã đổi. Chọn lại ảnh để tạo bản mới.".into());
    }
    settings.logo.path = save_image(&dir, &bytes, ext, &hash)?
        .to_string_lossy()
        .into_owned();
    Ok(())
}
