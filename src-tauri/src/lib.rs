use arboard::{Clipboard, ImageData};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use image::{DynamicImage, ImageFormat, RgbaImage};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::borrow::Cow;
use std::fs;
use std::fs::OpenOptions;
use std::io::Cursor;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;
use std::time::Instant;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, PhysicalPosition, Position, State, WebviewWindow, WindowEvent};
use tauri_plugin_autostart::ManagerExt as AutostartExt;
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use tauri_plugin_opener::OpenerExt;

const HISTORY_FILE_NAME: &str = "clipboard-history.json";
const SETTINGS_FILE_NAME: &str = "settings.json";
const IMAGE_DIR_NAME: &str = "clipboard-images";
const LOG_FILE_NAME: &str = "clipboard-history.log";
const AUTOSTART_LAUNCH_ARG: &str = "--autostart";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppSettings {
    poll_interval_ms: u64,
    history_limit: usize,
    storage_dir: String,
    global_shortcut: String,
    launch_at_startup: bool,
    always_on_top: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            poll_interval_ms: 800,
            history_limit: 300,
            storage_dir: String::new(),
            global_shortcut: "Alt+Shift+V".to_string(),
            launch_at_startup: false,
            always_on_top: false,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateSettingsPayload {
    poll_interval_ms: Option<u64>,
    history_limit: Option<usize>,
    storage_dir: Option<String>,
    global_shortcut: Option<String>,
    launch_at_startup: Option<bool>,
    always_on_top: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ClipboardItem {
    id: String,
    #[serde(rename = "type")]
    item_type: String,
    text: Option<String>,
    #[serde(rename = "imagePath")]
    image_path: Option<String>,
    #[serde(rename = "imagePreviewDataUrl")]
    image_preview_data_url: Option<String>,
    #[serde(rename = "contentHash")]
    content_hash: String,
    #[serde(rename = "isFavorite")]
    is_favorite: bool,
    #[serde(rename = "createdAt")]
    created_at: u64,
    #[serde(rename = "updatedAt")]
    updated_at: u64,
}

struct AppState {
    last_capture_fingerprint: Mutex<Option<String>>,
    history_lock: Mutex<()>,
    last_diagnostic_log_at: Mutex<u64>,
    suppress_auto_hide_until: Mutex<u64>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            last_capture_fingerprint: Mutex::new(None),
            history_lock: Mutex::new(()),
            last_diagnostic_log_at: Mutex::new(0),
            suppress_auto_hide_until: Mutex::new(0),
        }
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|dur| dur.as_millis() as u64)
        .unwrap_or(0)
}

fn launched_from_autostart() -> bool {
    std::env::args().any(|arg| arg == AUTOSTART_LAUNCH_ARG)
}

fn hash_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn normalize_text(text: &str) -> String {
    text.replace("\r\n", "\n").trim().to_string()
}

fn text_preview_for_log(text: &str, max_chars: usize) -> String {
    let compact = text.replace(['\r', '\n', '\t'], " ");
    let mut preview = String::new();
    for ch in compact.chars().take(max_chars) {
        preview.push(ch);
    }
    preview
}

fn is_internal_log_text(text: &str) -> bool {
    let normalized = text.trim();
    if normalized.is_empty() {
        return false;
    }

    normalized.contains("[INFO] history updated with")
        || normalized.contains("[INFO] fallback to text capture")
        || normalized.contains("[INFO] ignored internal log text in clipboard")
        || normalized.contains("source=text-fallback")
        || normalized.contains("[DEBUG] poll no-capture")
        || normalized.contains("plugin_image=")
        || normalized.contains("arboard_image=")
}

fn sanitize_shortcut(shortcut: &str) -> String {
    let mut parts: Vec<String> = Vec::new();
    for part in shortcut.split('+') {
        let p = part.trim();
        if p.eq_ignore_ascii_case("meta") {
            parts.push("Super".to_string());
        } else if p.eq_ignore_ascii_case("cmdorctrl") {
            parts.push("CommandOrControl".to_string());
        } else if !p.is_empty() {
            parts.push(p.to_string());
        }
    }
    if parts.is_empty() {
        "Alt+Shift+V".to_string()
    } else {
        parts.join("+")
    }
}

fn normalize_settings(mut settings: AppSettings) -> AppSettings {
    settings.poll_interval_ms = settings.poll_interval_ms.clamp(300, 5000);
    settings.history_limit = settings.history_limit.clamp(50, 5000);
    settings.storage_dir = settings.storage_dir.trim().to_string();
    settings.global_shortcut = sanitize_shortcut(&settings.global_shortcut);
    if settings.global_shortcut.is_empty() {
        settings.global_shortcut = "Alt+Shift+V".to_string();
    }
    settings
}

fn set_always_on_top(app: &AppHandle, enabled: bool) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "未找到主窗口".to_string())?;

    window
        .set_always_on_top(enabled)
        .map_err(|e| format!("设置窗口置顶失败: {e}"))
}

fn set_autostart_enabled(app: &AppHandle, enabled: bool) -> Result<(), String> {
    let manager = app.autolaunch();
    let current = manager
        .is_enabled()
        .map_err(|e| format!("读取开机自启状态失败: {e}"))?;

    if current == enabled {
        return Ok(());
    }

    if enabled {
        manager
            .enable()
            .map_err(|e| format!("启用开机自启失败: {e}"))?;
    } else {
        manager
            .disable()
            .map_err(|e| format!("禁用开机自启失败: {e}"))?;
    }

    Ok(())
}

fn toggle_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        if window.is_visible().unwrap_or(false) {
            let _ = window.hide();
        } else {
            let _ = window.show();
            let _ = window.set_focus();
        }
    }
}

fn clamp_window_position_to_monitor(
    app: &AppHandle,
    window: &WebviewWindow,
    desired_x: i32,
    desired_y: i32,
) -> Option<(i32, i32)> {
    let monitor = app
        .monitor_from_point(f64::from(desired_x), f64::from(desired_y))
        .ok()
        .flatten()
        .or_else(|| window.current_monitor().ok().flatten())?;

    let monitor_pos = monitor.position();
    let monitor_size = monitor.size();
    let window_size = window.outer_size().ok()?;

    let min_x = i64::from(monitor_pos.x);
    let min_y = i64::from(monitor_pos.y);
    let max_x = (min_x + i64::from(monitor_size.width) - i64::from(window_size.width)).max(min_x);
    let max_y = (min_y + i64::from(monitor_size.height) - i64::from(window_size.height)).max(min_y);

    let clamped_x = i64::from(desired_x).clamp(min_x, max_x) as i32;
    let clamped_y = i64::from(desired_y).clamp(min_y, max_y) as i32;
    Some((clamped_x, clamped_y))
}

fn show_main_window_at_cursor(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        if window.is_visible().unwrap_or(false) {
            let _ = window.hide();
            return;
        }

        if let Ok(cursor) = app.cursor_position() {
            let x = (cursor.x.round() as i32).saturating_add(12);
            let y = (cursor.y.round() as i32).saturating_add(12);
            let (target_x, target_y) =
                clamp_window_position_to_monitor(app, &window, x, y).unwrap_or((x, y));
            let _ = window.set_position(Position::Physical(PhysicalPosition::new(
                target_x, target_y,
            )));
        }
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn position_main_window_bottom_right(app: &AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "未找到主窗口".to_string())?;
    let monitor = window
        .current_monitor()
        .map_err(|e| format!("读取当前显示器失败: {e}"))?;

    let Some(monitor) = monitor else {
        return Ok(());
    };

    let monitor_pos = monitor.position();
    let monitor_size = monitor.size();
    let window_size = window
        .outer_size()
        .map_err(|e| format!("读取窗口尺寸失败: {e}"))?;
    let margin = 16_i64;

    let target_x = (i64::from(monitor_pos.x) + i64::from(monitor_size.width)
        - i64::from(window_size.width)
        - margin)
        .max(i64::from(monitor_pos.x));
    let target_y = (i64::from(monitor_pos.y) + i64::from(monitor_size.height)
        - i64::from(window_size.height)
        - margin)
        .max(i64::from(monitor_pos.y));

    window
        .set_position(Position::Physical(PhysicalPosition::new(
            target_x as i32,
            target_y as i32,
        )))
        .map_err(|e| format!("设置默认窗口位置失败: {e}"))
}

fn setup_tray(app: &AppHandle) -> Result<(), String> {
    let toggle_item = MenuItem::with_id(app, "toggle", "显示/隐藏", true, None::<&str>)
        .map_err(|e| format!("创建托盘菜单失败: {e}"))?;
    let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)
        .map_err(|e| format!("创建托盘菜单失败: {e}"))?;
    let menu = Menu::with_items(app, &[&toggle_item, &quit_item])
        .map_err(|e| format!("创建托盘菜单失败: {e}"))?;

    let icon = app
        .default_window_icon()
        .ok_or_else(|| "未找到窗口图标，无法初始化托盘图标".to_string())?;

    TrayIconBuilder::new()
        .tooltip("Clipboard History")
        .icon(icon.clone())
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "toggle" => toggle_main_window(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_main_window(&tray.app_handle());
            }
        })
        .build(app)
        .map_err(|e| format!("初始化托盘失败: {e}"))?;

    Ok(())
}

fn register_global_shortcut(app: &AppHandle, accelerator: &str) -> Result<(), String> {
    let shortcut: Shortcut = accelerator
        .parse()
        .map_err(|e| format!("快捷键格式无效: {e}"))?;

    app.global_shortcut()
        .unregister_all()
        .map_err(|e| format!("清理旧快捷键失败: {e}"))?;

    app.global_shortcut()
        .register(shortcut)
        .map_err(|e| format!("注册快捷键失败: {e}"))
}

fn app_root_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let path = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("无法定位应用数据目录: {e}"))?;
    fs::create_dir_all(&path).map_err(|e| format!("创建应用数据目录失败: {e}"))?;
    Ok(path)
}

fn settings_file(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app_root_dir(app)?.join(SETTINGS_FILE_NAME))
}

fn append_log(app: &AppHandle, level: &str, message: &str) {
    let Ok(path) = app_root_dir(app).map(|dir| dir.join(LOG_FILE_NAME)) else {
        return;
    };

    let line = format!("[{}] [{}] {}\n", now_ms(), level, message);
    let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) else {
        return;
    };
    let _ = file.write_all(line.as_bytes());
}

fn append_diagnostic_log_throttled(app: &AppHandle, state: &State<AppState>, message: &str) {
    let now = now_ms();
    let Ok(mut last) = state.last_diagnostic_log_at.lock() else {
        return;
    };

    if now.saturating_sub(*last) < 3000 {
        return;
    }

    *last = now;
    append_log(app, "DEBUG", message);
}

fn load_settings(app: &AppHandle) -> Result<AppSettings, String> {
    let path = settings_file(app)?;
    if !path.exists() {
        return Ok(AppSettings::default());
    }

    let raw = fs::read_to_string(path).map_err(|e| format!("读取设置失败: {e}"))?;
    let parsed = serde_json::from_str::<AppSettings>(&raw).unwrap_or_default();
    Ok(normalize_settings(parsed))
}

fn save_settings(app: &AppHandle, settings: &AppSettings) -> Result<(), String> {
    let path = settings_file(app)?;
    let json =
        serde_json::to_string_pretty(settings).map_err(|e| format!("序列化设置失败: {e}"))?;
    fs::write(path, json).map_err(|e| format!("写入设置失败: {e}"))
}

fn data_dir_from_settings(app: &AppHandle, settings: &AppSettings) -> Result<PathBuf, String> {
    let path = if settings.storage_dir.is_empty() {
        app_root_dir(app)?
    } else {
        PathBuf::from(&settings.storage_dir)
    };
    fs::create_dir_all(&path).map_err(|e| format!("创建数据目录失败: {e}"))?;
    Ok(path)
}

fn data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let settings = load_settings(app)?;
    data_dir_from_settings(app, &settings)
}

fn image_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = data_dir(app)?.join(IMAGE_DIR_NAME);
    fs::create_dir_all(&dir).map_err(|e| format!("创建图片目录失败: {e}"))?;
    Ok(dir)
}

fn history_file(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(data_dir(app)?.join(HISTORY_FILE_NAME))
}

fn ensure_storage_layout(app: &AppHandle) -> Result<(), String> {
    let base = data_dir(app)?;
    fs::create_dir_all(base.join(IMAGE_DIR_NAME)).map_err(|e| format!("创建图片目录失败: {e}"))?;

    let history = base.join(HISTORY_FILE_NAME);
    if !history.exists() {
        fs::write(&history, "[]").map_err(|e| format!("初始化历史文件失败: {e}"))?;
    }
    Ok(())
}

fn migrate_storage_if_needed(old_dir: &Path, new_dir: &Path) -> Result<(), String> {
    if old_dir == new_dir {
        return Ok(());
    }

    fs::create_dir_all(new_dir).map_err(|e| format!("创建新目录失败: {e}"))?;

    let old_history = old_dir.join(HISTORY_FILE_NAME);
    let new_history = new_dir.join(HISTORY_FILE_NAME);
    if old_history.exists() && !new_history.exists() {
        fs::copy(&old_history, &new_history).map_err(|e| format!("迁移历史文件失败: {e}"))?;
    }

    let old_images = old_dir.join(IMAGE_DIR_NAME);
    let new_images = new_dir.join(IMAGE_DIR_NAME);
    if old_images.exists() {
        fs::create_dir_all(&new_images).map_err(|e| format!("创建新图片目录失败: {e}"))?;
        let entries = fs::read_dir(&old_images).map_err(|e| format!("读取旧图片目录失败: {e}"))?;
        for entry in entries {
            let entry = entry.map_err(|e| format!("读取目录项失败: {e}"))?;
            let from = entry.path();
            if from.is_file() {
                let to = new_images.join(entry.file_name());
                if !to.exists() {
                    fs::copy(&from, &to).map_err(|e| format!("迁移图片文件失败: {e}"))?;
                }
            }
        }
    }

    Ok(())
}

fn load_history(app: &AppHandle) -> Result<Vec<ClipboardItem>, String> {
    let path = history_file(app)?;
    if !path.exists() {
        return Ok(Vec::new());
    }

    let raw = fs::read_to_string(&path).map_err(|e| format!("读取历史失败: {e}"))?;
    let mut items: Vec<ClipboardItem> =
        serde_json::from_str(&raw).map_err(|e| format!("解析历史失败: {e}"))?;
    items.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(items)
}

fn clean_history(items: Vec<ClipboardItem>, history_limit: usize) -> Vec<ClipboardItem> {
    let mut sorted = items;
    sorted.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

    let mut cleaned: Vec<ClipboardItem> = Vec::new();
    for mut item in sorted {
        if item.item_type == "text" {
            if let Some(text) = item.text.as_deref() {
                item.text = Some(normalize_text(text));
            }
        }

        if let Some(idx) = cleaned
            .iter()
            .position(|it| it.item_type == item.item_type && it.content_hash == item.content_hash)
        {
            if item.updated_at > cleaned[idx].updated_at {
                let mut keep = item;
                keep.updated_at = keep.updated_at.max(cleaned[idx].updated_at);
                keep.is_favorite = keep.is_favorite || cleaned[idx].is_favorite;
                cleaned[idx] = keep;
            } else {
                cleaned[idx].is_favorite = cleaned[idx].is_favorite || item.is_favorite;
            }
        } else {
            cleaned.push(item);
        }
    }

    cleaned.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    if cleaned.len() > history_limit {
        cleaned.truncate(history_limit);
    }
    cleaned
}

fn load_history_clean(app: &AppHandle) -> Result<Vec<ClipboardItem>, String> {
    let settings = load_settings(app)?;
    let items = load_history(app)?;
    let mut cleaned = clean_history(items, settings.history_limit);
    for item in &mut cleaned {
        item.image_preview_data_url = None;
    }
    Ok(cleaned)
}

fn build_image_preview_data_url(
    app: &AppHandle,
    item: &ClipboardItem,
) -> Result<Option<String>, String> {
    if item.item_type != "image" {
        return Ok(None);
    }

    let Some(rel) = item.image_path.as_deref() else {
        return Ok(None);
    };

    let path = data_dir(app)?.join(rel);
    let bytes = fs::read(path).map_err(|e| format!("读取图片预览失败: {e}"))?;
    Ok(Some(format!(
        "data:image/png;base64,{}",
        BASE64.encode(bytes)
    )))
}

fn save_history(app: &AppHandle, items: &[ClipboardItem]) -> Result<(), String> {
    let path = history_file(app)?;
    let to_store: Vec<ClipboardItem> = items
        .iter()
        .cloned()
        .map(|mut item| {
            item.image_preview_data_url = None;
            item
        })
        .collect();
    let json =
        serde_json::to_string_pretty(&to_store).map_err(|e| format!("序列化历史失败: {e}"))?;
    fs::write(path, json).map_err(|e| format!("写入历史失败: {e}"))
}

fn encode_rgba_to_png_bytes(image: &ImageData<'_>) -> Result<Vec<u8>, String> {
    let rgba = RgbaImage::from_raw(
        image.width as u32,
        image.height as u32,
        image.bytes.clone().into_owned(),
    )
    .ok_or_else(|| "图片像素格式无效".to_string())?;

    let mut cursor = Cursor::new(Vec::<u8>::new());
    DynamicImage::ImageRgba8(rgba)
        .write_to(&mut cursor, ImageFormat::Png)
        .map_err(|e| format!("编码 PNG 失败: {e}"))?;
    Ok(cursor.into_inner())
}

fn encode_rgba_raw_to_png_bytes(width: u32, height: u32, rgba: Vec<u8>) -> Result<Vec<u8>, String> {
    let image = ImageData {
        width: width as usize,
        height: height as usize,
        bytes: Cow::Owned(rgba),
    };
    encode_rgba_to_png_bytes(&image)
}

fn encode_dynamic_to_png_bytes(image: DynamicImage) -> Result<Vec<u8>, String> {
    let mut cursor = Cursor::new(Vec::<u8>::new());
    image
        .write_to(&mut cursor, ImageFormat::Png)
        .map_err(|e| format!("编码 PNG 失败: {e}"))?;
    Ok(cursor.into_inner())
}

fn image_item_from_png_bytes(app: &AppHandle, png_bytes: Vec<u8>) -> Result<ClipboardItem, String> {
    let content_hash = hash_bytes(&png_bytes);
    let now = now_ms();
    let file_name = format!("{hash}.png", hash = &content_hash[0..24]);
    let relative_path = format!("{IMAGE_DIR_NAME}/{file_name}");
    let full_path = image_dir(app)?.join(&file_name);
    let is_new_file = !full_path.exists();
    if is_new_file {
        fs::write(&full_path, &png_bytes).map_err(|e| format!("保存图片失败: {e}"))?;
    }

    let preview = if is_new_file {
        Some(format!(
            "data:image/png;base64,{}",
            BASE64.encode(&png_bytes)
        ))
    } else {
        None
    };

    Ok(ClipboardItem {
        id: format!("img-{now}-{suffix}", suffix = &content_hash[0..8]),
        item_type: "image".to_string(),
        text: None,
        image_path: Some(relative_path),
        image_preview_data_url: preview,
        content_hash,
        is_favorite: false,
        created_at: now,
        updated_at: now,
    })
}

fn image_item_from_rgba_bytes(
    app: &AppHandle,
    width: u32,
    height: u32,
    rgba: Vec<u8>,
) -> Result<ClipboardItem, String> {
    let png_bytes = encode_rgba_raw_to_png_bytes(width, height, rgba)?;
    image_item_from_png_bytes(app, png_bytes)
}

fn image_item_from_path(app: &AppHandle, path: &Path) -> Option<ClipboardItem> {
    if !path.exists() || !path.is_file() {
        return None;
    }

    let raw = fs::read(path).ok()?;
    let dyn_img = image::load_from_memory(&raw).ok()?;
    let png_bytes = encode_dynamic_to_png_bytes(dyn_img).ok()?;
    image_item_from_png_bytes(app, png_bytes).ok()
}

fn file_url_to_path(url: &str) -> Option<PathBuf> {
    let candidate = url.trim();

    let decoded = if let Some(rest) = candidate.strip_prefix("file:///") {
        Some(rest)
    } else if let Some(rest) = candidate.strip_prefix("file://") {
        Some(rest.strip_prefix("localhost/").unwrap_or(rest))
    } else {
        None
    }?;

    let normalized = decoded.replace('/', "\\");
    Some(PathBuf::from(normalized))
}

fn first_img_src(html: &str) -> Option<&str> {
    let lower = html.to_ascii_lowercase();
    let src_pos = lower.find("src=")?;
    let rest = html.get(src_pos + 4..)?.trim_start();
    if rest.is_empty() {
        return None;
    }

    let first_char = rest.chars().next()?;
    if first_char == '"' || first_char == '\'' {
        let tail = &rest[first_char.len_utf8()..];
        let end = tail.find(first_char)?;
        return Some(&tail[..end]);
    }

    let end = rest
        .find(|c: char| c.is_whitespace() || c == '>')
        .unwrap_or(rest.len());
    Some(&rest[..end])
}

fn try_image_item_from_text_source(app: &AppHandle, text: &str) -> Option<ClipboardItem> {
    let normalized = text.trim();

    let normalized = normalized
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(normalized)
        .trim();

    let normalized = if let Some(rest) = normalized.strip_prefix("file:///") {
        rest.replace('/', "\\")
    } else {
        normalized.to_string()
    };

    let normalized = normalized.as_str();

    if normalized.starts_with("data:image/") {
        let (_, payload) = normalized.split_once(',')?;
        let raw = BASE64.decode(payload).ok()?;
        let dyn_img = image::load_from_memory(&raw).ok()?;
        let png_bytes = encode_dynamic_to_png_bytes(dyn_img).ok()?;
        return image_item_from_png_bytes(app, png_bytes).ok();
    }

    if normalized.contains("<img") {
        if let Some(src) = first_img_src(normalized) {
            if src.starts_with("data:image/") {
                let (_, payload) = src.split_once(',')?;
                let raw = BASE64.decode(payload).ok()?;
                let dyn_img = image::load_from_memory(&raw).ok()?;
                let png_bytes = encode_dynamic_to_png_bytes(dyn_img).ok()?;
                return image_item_from_png_bytes(app, png_bytes).ok();
            }

            if let Some(path) = file_url_to_path(src) {
                if let Some(item) = image_item_from_path(app, &path) {
                    return Some(item);
                }
            }

            if let Some(item) = image_item_from_path(app, Path::new(src)) {
                return Some(item);
            }
        }
    }

    if let Some(path) = file_url_to_path(normalized) {
        if let Some(item) = image_item_from_path(app, &path) {
            return Some(item);
        }
    }

    if let Some(item) = image_item_from_path(app, Path::new(normalized)) {
        return Some(item);
    }

    None
}

#[cfg(target_os = "windows")]
fn read_clipboard_image_win32() -> Option<(u32, u32, Vec<u8>)> {
    use std::ffi::c_void;
    extern "system" {
        fn OpenClipboard(h: *mut c_void) -> i32;
        fn CloseClipboard() -> i32;
        fn GetClipboardData(format: u32) -> *mut c_void;
        fn GlobalLock(hmem: *mut c_void) -> *mut c_void;
        fn GlobalUnlock(hmem: *mut c_void) -> i32;
        fn GlobalSize(hmem: *mut c_void) -> usize;
    }
    unsafe {
        if OpenClipboard(std::ptr::null_mut()) == 0 {
            return None;
        }
        let hmem = GetClipboardData(8);
        if hmem.is_null() {
            CloseClipboard();
            return None;
        }
        let ptr = GlobalLock(hmem);
        if ptr.is_null() {
            CloseClipboard();
            return None;
        }
        let size = GlobalSize(hmem);
        let data = std::slice::from_raw_parts(ptr as *const u8, size);
        if data.len() < 40 {
            GlobalUnlock(hmem);
            CloseClipboard();
            return None;
        }
        let header_size = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
        let width = i32::from_le_bytes([data[4], data[5], data[6], data[7]]);
        let height = i32::from_le_bytes([data[8], data[9], data[10], data[11]]);
        let bit_count = u16::from_le_bytes([data[14], data[15]]);
        let compression = u32::from_le_bytes([data[16], data[17], data[18], data[19]]);
        let abs_height = height.unsigned_abs();
        let top_down = height < 0;

        if width <= 0 || abs_height == 0 {
            GlobalUnlock(hmem);
            CloseClipboard();
            return None;
        }
        let w = width as u32;
        let h = abs_height;
        let pixel_offset = if compression == 3 && bit_count >= 16 {
            header_size + 12
        } else {
            header_size
        };
        if pixel_offset >= data.len() {
            GlobalUnlock(hmem);
            CloseClipboard();
            return None;
        }
        let pixels = &data[pixel_offset..];
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        match bit_count {
            32 => {
                let row_bytes = (w * 4) as usize;
                for y in 0..h {
                    let src_y = if top_down { y } else { h - 1 - y };
                    let src_off = (src_y as usize) * row_bytes;
                    let dst_off = (y as usize) * (w as usize) * 4;
                    for x in 0..w as usize {
                        let si = src_off + x * 4;
                        let di = dst_off + x * 4;
                        if si + 3 < pixels.len() && di + 3 < rgba.len() {
                            rgba[di] = pixels[si + 2];
                            rgba[di + 1] = pixels[si + 1];
                            rgba[di + 2] = pixels[si];
                            rgba[di + 3] = pixels[si + 3];
                        }
                    }
                }
                if rgba.chunks(4).all(|px| px[3] == 0) {
                    for chunk in rgba.chunks_mut(4) {
                        chunk[3] = 255;
                    }
                }
            }
            24 => {
                let row_bytes = ((w * 3 + 3) & !3) as usize;
                for y in 0..h {
                    let src_y = if top_down { y } else { h - 1 - y };
                    let src_off = (src_y as usize) * row_bytes;
                    let dst_off = (y as usize) * (w as usize) * 4;
                    for x in 0..w as usize {
                        let si = src_off + x * 3;
                        let di = dst_off + x * 4;
                        if si + 2 < pixels.len() && di + 3 < rgba.len() {
                            rgba[di] = pixels[si + 2];
                            rgba[di + 1] = pixels[si + 1];
                            rgba[di + 2] = pixels[si];
                            rgba[di + 3] = 255;
                        }
                    }
                }
            }
            _ => {
                GlobalUnlock(hmem);
                CloseClipboard();
                return None;
            }
        }
        GlobalUnlock(hmem);
        CloseClipboard();
        Some((w, h, rgba))
    }
}

#[cfg(not(target_os = "windows"))]
fn read_clipboard_image_win32() -> Option<(u32, u32, Vec<u8>)> {
    None
}

fn to_image_item(app: &AppHandle, image: &ImageData<'_>) -> Result<ClipboardItem, String> {
    let png_bytes = encode_rgba_to_png_bytes(image)?;
    image_item_from_png_bytes(app, png_bytes)
}

fn to_text_item(text: String) -> ClipboardItem {
    let normalized = normalize_text(&text);
    let now = now_ms();
    let content_hash = hash_bytes(normalized.as_bytes());

    ClipboardItem {
        id: format!("txt-{now}-{suffix}", suffix = &content_hash[0..8]),
        item_type: "text".to_string(),
        text: Some(normalized),
        image_path: None,
        image_preview_data_url: None,
        content_hash,
        is_favorite: false,
        created_at: now,
        updated_at: now,
    }
}

fn fingerprint(item: &ClipboardItem) -> String {
    format!("{}:{}", item.item_type, item.content_hash)
}

fn fingerprint_from_current_clipboard() -> Option<String> {
    let mut clipboard = Clipboard::new().ok()?;

    if let Ok(image) = clipboard.get_image() {
        if let Ok(png_bytes) = encode_rgba_to_png_bytes(&image) {
            let content_hash = hash_bytes(&png_bytes);
            return Some(format!("image:{content_hash}"));
        }
    }

    if let Some((w, h, rgba)) = read_clipboard_image_win32() {
        if let Ok(png_bytes) = encode_rgba_raw_to_png_bytes(w, h, rgba) {
            let content_hash = hash_bytes(&png_bytes);
            return Some(format!("image:{content_hash}"));
        }
    }

    if let Ok(text) = clipboard.get_text() {
        let normalized = normalize_text(&text);
        if !normalized.is_empty() && !is_internal_log_text(&normalized) {
            let content_hash = hash_bytes(normalized.as_bytes());
            return Some(format!("text:{content_hash}"));
        }
    }

    None
}

fn dedupe_and_upsert(
    items: &mut Vec<ClipboardItem>,
    incoming: ClipboardItem,
    history_limit: usize,
) {
    if let Some(idx) = items.iter().position(|it| {
        it.item_type == incoming.item_type && it.content_hash == incoming.content_hash
    }) {
        let mut merged = items.remove(idx);
        merged.updated_at = now_ms();
        merged.image_preview_data_url = incoming
            .image_preview_data_url
            .or(merged.image_preview_data_url);
        items.insert(0, merged);
    } else {
        items.insert(0, incoming);
    }

    if items.len() > history_limit {
        items.truncate(history_limit);
    }
}

fn load_image_for_clipboard(path: &Path) -> Result<ImageData<'static>, String> {
    let bytes = fs::read(path).map_err(|e| format!("读取图片失败: {e}"))?;
    let img = image::load_from_memory(&bytes).map_err(|e| format!("解析图片失败: {e}"))?;
    let rgba = img.to_rgba8();
    let width = rgba.width() as usize;
    let height = rgba.height() as usize;
    let raw = rgba.into_raw();

    Ok(ImageData {
        width,
        height,
        bytes: Cow::Owned(raw),
    })
}

#[tauri::command]
fn get_settings(app: AppHandle) -> Result<AppSettings, String> {
    ensure_storage_layout(&app)?;
    load_settings(&app)
}

#[tauri::command]
fn get_storage_dir_path(app: AppHandle) -> Result<String, String> {
    ensure_storage_layout(&app)?;
    let dir = data_dir(&app)?;
    Ok(dir.to_string_lossy().to_string())
}

#[tauri::command]
fn open_storage_dir(app: AppHandle) -> Result<(), String> {
    ensure_storage_layout(&app)?;
    let dir = data_dir(&app)?;
    app.opener()
        .open_path(dir.as_path().to_string_lossy().as_ref(), None::<&str>)
        .map_err(|e| format!("打开目录失败: {e}"))
}

#[tauri::command]
fn update_settings(payload: UpdateSettingsPayload, app: AppHandle) -> Result<AppSettings, String> {
    let current = load_settings(&app)?;
    let old_dir = data_dir_from_settings(&app, &current)?;

    let mut next = current;
    if let Some(v) = payload.poll_interval_ms {
        next.poll_interval_ms = v;
    }
    if let Some(v) = payload.history_limit {
        next.history_limit = v;
    }
    if let Some(v) = payload.storage_dir {
        next.storage_dir = v;
    }
    if let Some(v) = payload.global_shortcut {
        next.global_shortcut = v;
    }
    if let Some(v) = payload.launch_at_startup {
        next.launch_at_startup = v;
    }
    if let Some(v) = payload.always_on_top {
        next.always_on_top = v;
    }
    next = normalize_settings(next);

    save_settings(&app, &next)?;

    let new_dir = data_dir_from_settings(&app, &next)?;
    migrate_storage_if_needed(&old_dir, &new_dir)?;

    register_global_shortcut(&app, &next.global_shortcut)?;
    if let Err(err) = set_autostart_enabled(&app, next.launch_at_startup) {
        append_log(
            &app,
            "WARN",
            &format!("apply autostart setting failed: {err}"),
        );
    }
    if let Err(err) = set_always_on_top(&app, next.always_on_top) {
        append_log(
            &app,
            "WARN",
            &format!("apply always-on-top setting failed: {err}"),
        );
    }

    Ok(next)
}

#[tauri::command]
fn get_history(app: AppHandle) -> Result<Vec<ClipboardItem>, String> {
    ensure_storage_layout(&app)?;
    load_history_clean(&app)
}

#[tauri::command]
fn get_image_preview(id: String, app: AppHandle) -> Result<Option<String>, String> {
    ensure_storage_layout(&app)?;
    let item = load_history_clean(&app)?
        .into_iter()
        .find(|it| it.id == id)
        .ok_or_else(|| "未找到历史项".to_string())?;
    build_image_preview_data_url(&app, &item)
}

#[tauri::command]
fn poll_clipboard(app: AppHandle, state: State<AppState>) -> Result<Option<ClipboardItem>, String> {
    ensure_storage_layout(&app)?;
    let poll_started_at = Instant::now();
    let _guard = state
        .history_lock
        .lock()
        .map_err(|_| "历史锁获取失败".to_string())?;

    let mut clipboard = Clipboard::new().map_err(|e| format!("访问系统剪贴板失败: {e}"))?;

    let mut image_captured: Option<ClipboardItem> = None;
    let mut capture_source = "";
    let mut capture_debug = String::new();
    let mut plugin_image_available = false;
    let mut arboard_image_available = false;
    let mut file_list_count = 0usize;
    let mut html_len = 0usize;
    let mut text_len = 0usize;
    let mut plugin_error = String::new();
    let mut arboard_attempts = 0usize;
    let mut arboard_last_error = String::new();
    let mut plugin_to_arboard_waited = false;

    match app.clipboard().read_image() {
        Ok(img) => {
            plugin_image_available = true;
            match image_item_from_rgba_bytes(
                &app,
                img.width() as u32,
                img.height() as u32,
                img.rgba().to_vec(),
            ) {
                Ok(item) => {
                    capture_source = "plugin-image";
                    image_captured = Some(item);
                }
                Err(err) => {
                    plugin_error = err;
                }
            }
        }
        Err(err) => {
            plugin_error = err.to_string();
        }
    }

    if image_captured.is_none() {
        thread::sleep(Duration::from_millis(5));
        plugin_to_arboard_waited = true;
        for attempt in 0..2 {
            arboard_attempts = attempt + 1;
            match clipboard.get_image() {
                Ok(image) => {
                    arboard_image_available = true;
                    image_captured = Some(to_image_item(&app, &image)?);
                    capture_source = "arboard-image";
                    break;
                }
                Err(err) => {
                    arboard_last_error = err.to_string();
                }
            }
            if attempt < 1 {
                thread::sleep(Duration::from_millis(10));
            }
        }
    }

    if image_captured.is_none() {
        if let Some((w, h, rgba)) = read_clipboard_image_win32() {
            match image_item_from_rgba_bytes(&app, w, h, rgba) {
                Ok(item) => {
                    capture_source = "win32-dib";
                    image_captured = Some(item);
                }
                Err(_) => {}
            }
        }
    }

    let incoming = if let Some(item) = image_captured {
        Some(item)
    } else {
        let mut from_other_formats: Option<ClipboardItem> = None;

        if let Ok(paths) = clipboard.get().file_list() {
            file_list_count = paths.len();
            for path in paths {
                if let Some(item) = image_item_from_path(&app, &path) {
                    capture_source = "file-list-image";
                    capture_debug = path.display().to_string();
                    from_other_formats = Some(item);
                    break;
                }
            }
        }

        if from_other_formats.is_none() {
            if let Ok(html) = clipboard.get().html() {
                let normalized_html = normalize_text(&html);
                html_len = normalized_html.len();
                if !normalized_html.is_empty() {
                    if let Some(item) = try_image_item_from_text_source(&app, &normalized_html) {
                        capture_source = "html-image";
                        from_other_formats = Some(item);
                    }
                }
            }
        }

        if let Some(item) = from_other_formats {
            Some(item)
        } else if let Ok(text) = clipboard.get_text() {
            let normalized = normalize_text(&text);
            text_len = normalized.len();
            if normalized.is_empty() {
                None
            } else if is_internal_log_text(&normalized) {
                append_log(&app, "INFO", "ignored internal log text in clipboard");
                None
            } else if let Some(image_item) = try_image_item_from_text_source(&app, &normalized) {
                capture_source = "text-parsed-image";
                Some(image_item)
            } else {
                capture_source = "text-fallback";
                capture_debug = text_preview_for_log(&normalized, 120);
                Some(to_text_item(normalized))
            }
        } else {
            None
        }
    };

    let Some(item) = incoming else {
        append_diagnostic_log_throttled(
            &app,
            &state,
            &format!(
                "poll no-capture plugin_image={plugin_image_available} arboard_image={arboard_image_available} file_list_count={file_list_count} html_len={html_len} text_len={text_len}"
            ),
        );
        if !plugin_error.is_empty() || !arboard_last_error.is_empty() {
            append_diagnostic_log_throttled(
                &app,
                &state,
                &format!(
                    "poll no-capture details plugin_waited={plugin_to_arboard_waited} arboard_attempts={arboard_attempts} poll_ms={} plugin_error={} arboard_error={}",
                    poll_started_at.elapsed().as_millis(),
                    text_preview_for_log(&plugin_error, 120),
                    text_preview_for_log(&arboard_last_error, 120)
                ),
            );
        }
        return Ok(None);
    };

    let fp = fingerprint(&item);
    {
        let mut last = state
            .last_capture_fingerprint
            .lock()
            .map_err(|_| "指纹锁获取失败".to_string())?;
        if last.as_deref() == Some(&fp) {
            return Ok(None);
        }
        *last = Some(fp);
    }

    let settings = load_settings(&app)?;
    let mut items = load_history(&app)?;
    dedupe_and_upsert(&mut items, item, settings.history_limit);
    save_history(&app, &items)?;
    let item_type = &items[0].item_type;
    if capture_debug.is_empty() {
        append_log(
            &app,
            "INFO",
            &format!("history updated with {item_type} item, source={capture_source}"),
        );
    } else {
        append_log(
            &app,
            "INFO",
            &format!("history updated with {item_type} item, source={capture_source}, detail={capture_debug}"),
        );
    }
    let mut latest = items.first().cloned();
    if let Some(item) = &mut latest {
        item.image_preview_data_url = build_image_preview_data_url(&app, item).ok().flatten();
    }

    Ok(latest)
}

#[tauri::command]
fn copy_history_item(id: String, app: AppHandle, state: State<AppState>) -> Result<(), String> {
    let item = load_history_clean(&app)?
        .into_iter()
        .find(|it| it.id == id)
        .ok_or_else(|| "未找到历史项".to_string())?;

    let mut clipboard = Clipboard::new().map_err(|e| format!("访问系统剪贴板失败: {e}"))?;

    if item.item_type == "text" {
        let text = item.text.as_deref().unwrap_or_default().to_string();
        clipboard
            .set_text(text)
            .map_err(|e| format!("写入文本到剪贴板失败: {e}"))?;
    } else {
        let rel = item
            .image_path
            .as_deref()
            .ok_or_else(|| "图片路径缺失".to_string())?;
        let path = data_dir(&app)?.join(rel);
        let image = load_image_for_clipboard(&path)?;
        clipboard
            .set_image(image)
            .map_err(|e| format!("写入图片到剪贴板失败: {e}"))?;
    }

    let mut last = state
        .last_capture_fingerprint
        .lock()
        .map_err(|_| "指纹锁获取失败".to_string())?;
    *last = Some(fingerprint(&item));

    Ok(())
}

#[tauri::command]
fn copy_text(text: String) -> Result<(), String> {
    let mut clipboard = Clipboard::new().map_err(|e| format!("访问系统剪贴板失败: {e}"))?;
    clipboard
        .set_text(text)
        .map_err(|e| format!("写入文本到剪贴板失败: {e}"))
}

#[tauri::command]
fn toggle_favorite(id: String, app: AppHandle) -> Result<Option<ClipboardItem>, String> {
    let mut items = load_history_clean(&app)?;
    let mut updated: Option<ClipboardItem> = None;

    for item in &mut items {
        if item.id == id {
            item.is_favorite = !item.is_favorite;
            item.updated_at = now_ms();
            updated = Some(item.clone());
            break;
        }
    }

    if let Some(ref changed) = updated {
        items.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        if let Some(idx) = items.iter().position(|it| it.id == changed.id) {
            let item = items.remove(idx);
            items.insert(0, item);
        }
        save_history(&app, &items)?;
    }

    Ok(updated)
}

#[tauri::command]
fn delete_history_item(id: String, app: AppHandle, state: State<AppState>) -> Result<(), String> {
    let _guard = state
        .history_lock
        .lock()
        .map_err(|_| "历史锁获取失败".to_string())?;

    let mut items = load_history_clean(&app)?;
    let idx = items
        .iter()
        .position(|it| it.id == id)
        .ok_or_else(|| "未找到历史项".to_string())?;
    let removed = items.remove(idx);

    if removed.item_type == "image" {
        if let Some(rel) = removed.image_path.as_deref() {
            let path = data_dir(&app)?.join(rel);
            if path.exists() {
                fs::remove_file(path).map_err(|e| format!("删除图片失败: {e}"))?;
            }
        }
    }

    save_history(&app, &items)?;

    let mut last = state
        .last_capture_fingerprint
        .lock()
        .map_err(|_| "指纹锁获取失败".to_string())?;
    if last.as_deref() == Some(&fingerprint(&removed)) {
        *last = None;
    }

    Ok(())
}

#[tauri::command]
fn clear_history(app: AppHandle, state: State<AppState>) -> Result<(), String> {
    let _guard = state
        .history_lock
        .lock()
        .map_err(|_| "历史锁获取失败".to_string())?;

    save_history(&app, &[])?;

    let img_dir = image_dir(&app)?;
    if img_dir.exists() {
        let entries = fs::read_dir(&img_dir).map_err(|e| format!("读取图片目录失败: {e}"))?;
        for entry in entries {
            let entry = entry.map_err(|e| format!("读取目录项失败: {e}"))?;
            let path = entry.path();
            if path.is_file() {
                fs::remove_file(path).map_err(|e| format!("删除图片失败: {e}"))?;
            }
        }
    }

    let current_fingerprint = fingerprint_from_current_clipboard();

    let mut last = state
        .last_capture_fingerprint
        .lock()
        .map_err(|_| "指纹锁获取失败".to_string())?;
    *last = current_fingerprint;

    Ok(())
}

#[tauri::command]
fn suppress_auto_hide(state: State<AppState>) -> Result<(), String> {
    let mut until = state
        .suppress_auto_hide_until
        .lock()
        .map_err(|_| "自动隐藏抑制锁获取失败".to_string())?;
    *until = now_ms().saturating_add(1500);
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .arg(AUTOSTART_LAUNCH_ARG)
                .build(),
        )
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        show_main_window_at_cursor(app);
                    }
                })
                .build(),
        )
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let silent_start = launched_from_autostart();
            setup_tray(&app.handle())?;
            if let Err(err) = position_main_window_bottom_right(&app.handle()) {
                append_log(
                    &app.handle(),
                    "WARN",
                    &format!("setup default window position failed: {err}"),
                );
            }
            if let Some(window) = app.get_webview_window("main") {
                if silent_start {
                    let _ = window.hide();
                } else {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            ensure_storage_layout(&app.handle())?;
            let settings = load_settings(&app.handle())?;
            if let Err(err) = register_global_shortcut(&app.handle(), &settings.global_shortcut) {
                eprintln!("global shortcut setup failed: {err}");
                let fallback = "Alt+Shift+V";
                register_global_shortcut(&app.handle(), fallback)?;
            }
            if let Err(err) = set_autostart_enabled(&app.handle(), settings.launch_at_startup) {
                append_log(
                    &app.handle(),
                    "WARN",
                    &format!("setup autostart failed: {err}"),
                );
            }
            if let Err(err) = set_always_on_top(&app.handle(), settings.always_on_top) {
                append_log(
                    &app.handle(),
                    "WARN",
                    &format!("setup always-on-top failed: {err}"),
                );
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
                return;
            }

            if let WindowEvent::Focused(false) = event {
                if let Ok(until) = window
                    .app_handle()
                    .state::<AppState>()
                    .suppress_auto_hide_until
                    .lock()
                {
                    if now_ms() < *until {
                        return;
                    }
                }
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            get_storage_dir_path,
            open_storage_dir,
            update_settings,
            get_history,
            get_image_preview,
            poll_clipboard,
            copy_history_item,
            copy_text,
            toggle_favorite,
            delete_history_item,
            clear_history,
            suppress_auto_hide
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
