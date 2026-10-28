use std::path::{Path, PathBuf};
use std::sync::OnceLock;

static RESOURCES: OnceLock<PathBuf> = OnceLock::new();

pub(crate) fn initialize(app: &tauri::AppHandle) {
    use tauri::Manager;
    if let Ok(path) = app.path().resource_dir() {
        let _ = RESOURCES.set(path);
    }
}

pub(crate) fn data_root() -> PathBuf {
    if cfg!(debug_assertions) {
        return PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    }
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("com.khung.studio")
}

fn choose_tool(data: &Path, exe: &Path, resources: &Path, relative: &str) -> PathBuf {
    [
        data.join(relative),
        exe.join(relative),
        resources.join(relative),
    ]
    .into_iter()
    .find(|p| p.is_dir())
    .unwrap_or_else(|| data.join(relative))
}

// Only this app's data, executable directory and packaged resources are searched.
pub(crate) fn tool_dir(relative: &str) -> PathBuf {
    if cfg!(debug_assertions) {
        return data_root().join(relative);
    }
    let exe = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_default();
    let resources = RESOURCES.get().cloned().unwrap_or_else(|| exe.clone());
    choose_tool(&data_root(), &exe, &resources, relative)
}

pub(crate) fn resource(relative: &str) -> PathBuf {
    if cfg!(debug_assertions) {
        if relative.starts_with("douyin/") {
            return PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative);
        }
        return data_root().join(relative);
    }
    RESOURCES
        .get()
        .cloned()
        .or_else(|| {
            std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(Path::to_path_buf))
        })
        .unwrap_or_default()
        .join(relative)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn packaged_tools_do_not_depend_on_build_machine_paths() {
        let root = std::env::temp_dir().join(format!("runtime-paths-{}", std::process::id()));
        let data = root.join("data");
        let exe = root.join("installed");
        let resources = root.join("resources");
        std::fs::create_dir_all(exe.join("voices/vi")).unwrap();
        assert_eq!(
            choose_tool(&data, &exe, &resources, "voices/vi"),
            exe.join("voices/vi")
        );
        std::fs::create_dir_all(data.join("voices/vi")).unwrap();
        assert_eq!(
            choose_tool(&data, &exe, &resources, "voices/vi"),
            data.join("voices/vi")
        );
        assert_eq!(
            choose_tool(&data, &exe, &resources, "whisper"),
            data.join("whisper")
        );
        let _ = std::fs::remove_dir_all(root);
    }
}
