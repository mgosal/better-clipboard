#[cfg(not(target_os = "macos"))]
compile_error!("Better Clipboard is macOS-only for now.");

use std::{
    borrow::Cow,
    env, fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use arboard::{Clipboard, ImageData};
use chrono::{DateTime, Local};
use core_foundation::{
    base::TCFType,
    boolean::CFBoolean,
    dictionary::{CFDictionary, CFDictionaryRef},
    string::CFString,
};
use core_graphics::{
    event::{CGEvent, CGEventFlags, CGEventTapLocation},
    event_source::{CGEventSource, CGEventSourceStateID},
};
use directories::ProjectDirs;
use eframe::egui::{self, Color32, CornerRadius, Key, RichText, Stroke, TextureHandle};
use global_hotkey::{
    GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState,
    hotkey::{Code, HotKey, Modifiers},
};
use image::{ImageBuffer, ImageReader, Rgba};
use objc2::{
    AnyThread, MainThreadMarker, Message,
    rc::Retained,
    runtime::{AnyObject, ProtocolObject},
};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationOptions, NSPasteboard, NSPasteboardTypeFileURL,
    NSPasteboardWriting, NSRunningApplication, NSSharingServicePicker, NSWorkspace,
};
use objc2_foundation::{NSArray, NSPoint, NSRect, NSRectEdge, NSSize, NSString, NSURL};
use serde::{Deserialize, Serialize};
use tray_icon::{
    TrayIcon, TrayIconBuilder,
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
};
use url::Url;
use uuid::Uuid;

const DEFAULT_HISTORY_LIMIT: usize = 100;
const MIN_HISTORY_LIMIT: usize = 10;
const MAX_HISTORY_LIMIT: usize = 1_000;
const COMPACT_VISIBLE_ITEMS: usize = 3;
const ROW_HEIGHT: f32 = 118.0;
const THUMBNAIL_SIZE: f32 = 74.0;
const TEXT_PREVIEW_HEIGHT: f32 = 62.0;
const PREVIEW_SCALE: f32 = 0.5;
const PREVIEW_MAX_IMAGE_WIDTH: f32 = 1_280.0;
const PREVIEW_MAX_IMAGE_HEIGHT: f32 = 900.0;
const HINT_CHIP_WIDTH: f32 = 58.0;
const HINT_CHIP_HEIGHT: f32 = 48.0;
const HINT_CHIP_GAP: f32 = 6.0;
const FOCUS_HIDE_GRACE: Duration = Duration::from_millis(180);
const CHANGE_COUNT_CHECK_INTERVAL: Duration = Duration::from_millis(100);
const PASTE_DELAY: Duration = Duration::from_millis(140);
const PERMISSION_PROMPT_DELAY: Duration = Duration::from_millis(700);
const COMPACT_HEIGHT: f32 = 474.0;
const EXPANDED_HEIGHT: f32 = 720.0;
const SETTINGS_HEIGHT: f32 = 300.0;
const APP_NAME: &str = "Better Clipboard";
const LAUNCH_AGENT_LABEL: &str = "com.mgosal.better-clipboard";
const TRAY_SHOW_ID: &str = "better-clipboard-show";
const TRAY_SETTINGS_ID: &str = "better-clipboard-settings";
const TRAY_QUIT_ID: &str = "better-clipboard-quit";

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> bool;
}

fn main() -> eframe::Result<()> {
    let settings = AppSettings::load().unwrap_or_else(|err| {
        eprintln!("Could not load settings: {err:#}");
        AppSettings::default()
    });
    let needs_permission_onboarding = !accessibility_permission_granted();

    let store = Arc::new(Mutex::new(
        HistoryStore::load(settings.history_limit).unwrap_or_else(|err| {
            eprintln!("Could not load clipboard history: {err:#}");
            HistoryStore::new(default_data_dir(), settings.history_limit)
        }),
    ));

    start_clipboard_watcher(Arc::clone(&store));

    let shortcut = register_shortcut()
        .map_err(|err| {
            eprintln!("Could not register global shortcut: {err:#}");
            err
        })
        .ok();

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([620.0, COMPACT_HEIGHT])
            .with_min_inner_size([620.0, 150.0])
            .with_max_inner_size([620.0, EXPANDED_HEIGHT])
            .with_resizable(false)
            .with_decorations(false)
            .with_window_level(if needs_permission_onboarding {
                egui::WindowLevel::Normal
            } else {
                egui::WindowLevel::AlwaysOnTop
            })
            .with_visible(needs_permission_onboarding)
            .with_title(APP_NAME),
        ..Default::default()
    };

    eframe::run_native(
        APP_NAME,
        native_options,
        Box::new(move |cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);
            Ok(Box::new(BetterClipboardApp::new(
                store,
                shortcut,
                cc.egui_ctx.clone(),
                settings,
                needs_permission_onboarding,
            )))
        }),
    )
}

fn register_shortcut() -> Result<ShortcutState> {
    let manager = GlobalHotKeyManager::new().context("create global shortcut manager")?;
    let option_space = HotKey::new(Some(Modifiers::ALT), Code::Space);
    let command_option_space = HotKey::new(Some(Modifiers::SUPER | Modifiers::ALT), Code::Space);
    let command_option_backslash =
        HotKey::new(Some(Modifiers::SUPER | Modifiers::ALT), Code::Backslash);

    manager
        .register(option_space)
        .context("register Option+Space shortcut")?;
    manager
        .register(command_option_space)
        .context("register Cmd+Option+Space shortcut")?;
    manager
        .register(command_option_backslash)
        .context("register Cmd+Option+Backslash shortcut")?;

    Ok(ShortcutState {
        _manager: manager,
        hotkey_ids: vec![
            option_space.id(),
            command_option_space.id(),
            command_option_backslash.id(),
        ],
    })
}

struct ShortcutState {
    _manager: GlobalHotKeyManager,
    hotkey_ids: Vec<u32>,
}

struct TrayState {
    _tray_icon: TrayIcon,
}

enum AppEvent {
    Hotkey(u32),
    TrayMenu(String),
}

fn setup_event_handlers(ctx: egui::Context) -> mpsc::Receiver<AppEvent> {
    let (sender, receiver) = mpsc::channel();

    let hotkey_sender = sender.clone();
    let hotkey_ctx = ctx.clone();
    GlobalHotKeyEvent::set_event_handler(Some(move |event: GlobalHotKeyEvent| {
        if event.state() == HotKeyState::Pressed {
            let _ = hotkey_sender.send(AppEvent::Hotkey(event.id()));
            hotkey_ctx.request_repaint();
        }
    }));

    let menu_sender = sender.clone();
    let menu_ctx = ctx.clone();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        let _ = menu_sender.send(AppEvent::TrayMenu(event.id().as_ref().to_owned()));
        menu_ctx.request_repaint();
    }));

    receiver
}

fn setup_tray_icon() -> Result<TrayState> {
    let tray_menu = Menu::new();
    let show_item = MenuItem::with_id(TRAY_SHOW_ID, "Show Better Clipboard", true, None);
    let settings_item = MenuItem::with_id(TRAY_SETTINGS_ID, "Settings", true, None);
    let quit_item = MenuItem::with_id(TRAY_QUIT_ID, "Quit Better Clipboard", true, None);
    let separator = PredefinedMenuItem::separator();
    tray_menu
        .append_items(&[&show_item, &settings_item, &separator, &quit_item])
        .context("build tray menu")?;

    let tray_icon = TrayIconBuilder::new()
        .with_menu(Box::new(tray_menu))
        .with_tooltip(APP_NAME)
        .with_title("📋")
        .with_menu_on_left_click(true)
        .build()
        .context("create tray icon")?;

    Ok(TrayState {
        _tray_icon: tray_icon,
    })
}

fn start_clipboard_watcher(store: Arc<Mutex<HistoryStore>>) {
    thread::spawn(move || {
        let mut clipboard = match Clipboard::new() {
            Ok(clipboard) => clipboard,
            Err(err) => {
                eprintln!("Could not open clipboard: {err:#}");
                return;
            }
        };
        let mut monitor = ClipboardMonitor::new();
        let mut last_hash = String::new();

        loop {
            if monitor.should_read_clipboard() {
                if let Some(snapshot) = read_clipboard(&mut clipboard) {
                    let snapshot_hash = snapshot.hash().to_owned();
                    if snapshot_hash != last_hash {
                        last_hash = snapshot_hash;
                        if let Ok(mut store) = store.lock() {
                            if let Err(err) = store.push(snapshot) {
                                eprintln!("Could not save clipboard item: {err:#}");
                            }
                        }
                    }
                }
            }
            thread::sleep(CHANGE_COUNT_CHECK_INTERVAL);
        }
    });
}

struct ClipboardMonitor {
    last_change_count: i64,
}

impl ClipboardMonitor {
    fn new() -> Self {
        Self {
            last_change_count: clipboard_change_count(),
        }
    }

    fn should_read_clipboard(&mut self) -> bool {
        let change_count = clipboard_change_count();
        let changed = self.last_change_count != change_count;
        self.last_change_count = change_count;
        changed
    }
}

fn clipboard_change_count() -> i64 {
    NSPasteboard::generalPasteboard().changeCount() as i64
}

fn read_clipboard(clipboard: &mut Clipboard) -> Option<ClipboardSnapshot> {
    if let Some(paths) = read_file_paths_from_pasteboard() {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"files:");
        for path in &paths {
            hasher.update(path.to_string_lossy().as_bytes());
            hasher.update(b"\0");
        }
        return Some(ClipboardSnapshot::Files {
            paths,
            hash: hasher.finalize().to_hex().to_string(),
        });
    }

    if let Ok(text) = clipboard.get_text() {
        let text = text.trim_end_matches('\0').to_owned();
        if !text.trim().is_empty() {
            let mut hasher = blake3::Hasher::new();
            hasher.update(b"text:");
            hasher.update(text.as_bytes());
            return Some(ClipboardSnapshot::Text {
                text,
                hash: hasher.finalize().to_hex().to_string(),
            });
        }
    }

    if let Ok(image) = clipboard.get_image() {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"image:");
        hasher.update(&(image.width as u64).to_le_bytes());
        hasher.update(&(image.height as u64).to_le_bytes());
        hasher.update(&image.bytes);
        return Some(ClipboardSnapshot::Image {
            width: image.width,
            height: image.height,
            rgba: image.bytes.into_owned(),
            hash: hasher.finalize().to_hex().to_string(),
        });
    }

    None
}

fn read_file_paths_from_pasteboard() -> Option<Vec<PathBuf>> {
    let pasteboard = NSPasteboard::generalPasteboard();
    let mut paths = Vec::new();

    if let Some(items) = pasteboard.pasteboardItems() {
        for item in items.iter() {
            if let Some(value) = item.stringForType(unsafe { NSPasteboardTypeFileURL }) {
                if let Some(path) = file_path_from_file_url_string(&value.to_string()) {
                    paths.push(path);
                }
            }
        }
    }

    if paths.is_empty() {
        if let Some(value) = pasteboard.stringForType(unsafe { NSPasteboardTypeFileURL }) {
            if let Some(path) = file_path_from_file_url_string(&value.to_string()) {
                paths.push(path);
            }
        }
    }

    paths.dedup();
    (!paths.is_empty()).then_some(paths)
}

fn file_path_from_file_url_string(value: &str) -> Option<PathBuf> {
    let url = Url::parse(value.trim()).ok()?;
    if url.scheme() != "file" {
        return None;
    }
    let path = url.to_file_path().ok()?;
    path.exists().then_some(path)
}

enum ClipboardSnapshot {
    Text {
        text: String,
        hash: String,
    },
    Files {
        paths: Vec<PathBuf>,
        hash: String,
    },
    Image {
        width: usize,
        height: usize,
        rgba: Vec<u8>,
        hash: String,
    },
}

impl ClipboardSnapshot {
    fn hash(&self) -> &str {
        match self {
            Self::Text { hash, .. } | Self::Files { hash, .. } | Self::Image { hash, .. } => hash,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct AppSettings {
    #[serde(default)]
    theme: ThemeMode,
    #[serde(default = "default_history_limit")]
    history_limit: usize,
    #[serde(default)]
    launch_at_login: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: ThemeMode::Dark,
            history_limit: DEFAULT_HISTORY_LIMIT,
            launch_at_login: launch_agent_enabled(),
        }
    }
}

fn default_history_limit() -> usize {
    DEFAULT_HISTORY_LIMIT
}

impl AppSettings {
    fn load() -> Result<Self> {
        let data_dir = default_data_dir();
        fs::create_dir_all(&data_dir).context("create settings directory")?;
        let path = settings_path(&data_dir);
        if !path.exists() {
            return Ok(Self::default());
        }

        let bytes = fs::read(path).context("read settings file")?;
        let mut settings: Self = serde_json::from_slice(&bytes).context("parse settings file")?;
        settings.history_limit = settings
            .history_limit
            .clamp(MIN_HISTORY_LIMIT, MAX_HISTORY_LIMIT);
        settings.launch_at_login = launch_agent_enabled();
        Ok(settings)
    }

    fn save(&self) -> Result<()> {
        let data_dir = default_data_dir();
        fs::create_dir_all(&data_dir).context("create settings directory")?;
        let bytes = serde_json::to_vec_pretty(self).context("serialize settings")?;
        fs::write(settings_path(&data_dir), bytes).context("write settings")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum ThemeMode {
    Light,
    Dark,
}

impl Default for ThemeMode {
    fn default() -> Self {
        Self::Dark
    }
}

impl ThemeMode {
    fn label(self) -> &'static str {
        match self {
            Self::Light => "Light",
            Self::Dark => "Dark",
        }
    }
}

fn apply_theme(ctx: &egui::Context, theme: ThemeMode) {
    match theme {
        ThemeMode::Light => ctx.set_visuals(egui::Visuals::light()),
        ThemeMode::Dark => ctx.set_visuals(egui::Visuals::dark()),
    }
}

#[derive(Debug, Clone)]
struct HistoryStore {
    data_dir: PathBuf,
    max_items: usize,
    items: Vec<ClipItem>,
    suppressed_hashes: Vec<String>,
}

impl HistoryStore {
    fn new(data_dir: PathBuf, max_items: usize) -> Self {
        Self {
            data_dir,
            max_items,
            items: Vec::new(),
            suppressed_hashes: Vec::new(),
        }
    }

    fn load(max_items: usize) -> Result<Self> {
        let data_dir = default_data_dir();
        fs::create_dir_all(images_dir(&data_dir)).context("create data directories")?;

        let history_path = history_path(&data_dir);
        if !history_path.exists() {
            return Ok(Self::new(data_dir, max_items));
        }

        let bytes = fs::read(&history_path).context("read history file")?;
        let mut items: Vec<ClipItem> =
            serde_json::from_slice(&bytes).context("parse history file")?;
        items.truncate(max_items);
        let changed = items.iter_mut().fold(false, |changed, item| {
            normalize_loaded_item(item) || changed
        });
        let store = Self {
            data_dir,
            max_items,
            items,
            suppressed_hashes: Vec::new(),
        };
        if changed {
            store.save().context("save normalized history")?;
        }
        Ok(store)
    }

    fn push(&mut self, snapshot: ClipboardSnapshot) -> Result<()> {
        let hash = snapshot.hash().to_owned();
        if let Some(index) = self
            .suppressed_hashes
            .iter()
            .position(|suppressed| suppressed == &hash)
        {
            self.suppressed_hashes.remove(index);
            return Ok(());
        }
        if self.items.first().is_some_and(|item| item.hash == hash) {
            return Ok(());
        }

        self.items.retain(|item| item.hash != hash);
        let item = match snapshot {
            ClipboardSnapshot::Text { text, hash } => ClipItem::from_text(text, hash),
            ClipboardSnapshot::Files { paths, hash } => ClipItem::from_files(paths, hash),
            ClipboardSnapshot::Image {
                width,
                height,
                rgba,
                hash,
            } => self.item_from_image(width, height, rgba, hash)?,
        };

        self.items.insert(0, item);
        self.prune_to_limit();
        self.save()
    }

    fn suppress_next_hash(&mut self, hash: &str) {
        if !self
            .suppressed_hashes
            .iter()
            .any(|suppressed| suppressed == hash)
        {
            self.suppressed_hashes.push(hash.to_owned());
        }
    }

    fn allow_hash(&mut self, hash: &str) {
        if let Some(index) = self
            .suppressed_hashes
            .iter()
            .position(|suppressed| suppressed == hash)
        {
            self.suppressed_hashes.remove(index);
        }
    }

    fn promote(&mut self, id: Uuid) -> Result<()> {
        let Some(index) = self.items.iter().position(|item| item.id == id) else {
            return Ok(());
        };
        if index == 0 {
            return Ok(());
        }

        let item = self.items.remove(index);
        self.items.insert(0, item);
        self.save()
    }

    fn set_max_items(&mut self, max_items: usize) -> Result<()> {
        self.max_items = max_items.clamp(MIN_HISTORY_LIMIT, MAX_HISTORY_LIMIT);
        self.prune_to_limit();
        self.save()
    }

    fn prune_to_limit(&mut self) {
        if self.items.len() <= self.max_items {
            return;
        }

        let removed = self.items.split_off(self.max_items);
        for item in removed {
            if let Some(image) = item.image {
                let _ = fs::remove_file(self.data_dir.join(image.path));
            }
        }
    }

    fn save(&self) -> Result<()> {
        fs::create_dir_all(images_dir(&self.data_dir)).context("create data directories")?;
        let bytes = serde_json::to_vec_pretty(&self.items).context("serialize history")?;
        fs::write(history_path(&self.data_dir), bytes).context("write history")
    }

    fn item_from_image(
        &self,
        width: usize,
        height: usize,
        rgba: Vec<u8>,
        hash: String,
    ) -> Result<ClipItem> {
        fs::create_dir_all(images_dir(&self.data_dir)).context("create images directory")?;
        let file_name = format!("{}.png", Uuid::new_v4());
        let relative = PathBuf::from("images").join(file_name);
        let absolute = self.data_dir.join(&relative);

        let buffer = ImageBuffer::<Rgba<u8>, Vec<u8>>::from_raw(width as u32, height as u32, rgba)
            .context("build image buffer from clipboard bytes")?;
        buffer.save(&absolute).context("save clipboard image")?;

        Ok(ClipItem {
            id: Uuid::new_v4(),
            kind: ClipKind::Image,
            summary: format!("{width} x {height} image"),
            text: None,
            image: Some(ImageClip {
                path: relative,
                width,
                height,
            }),
            files: Vec::new(),
            created_at: Local::now(),
            hash,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ClipItem {
    id: Uuid,
    kind: ClipKind,
    summary: String,
    text: Option<String>,
    image: Option<ImageClip>,
    #[serde(default)]
    files: Vec<PathBuf>,
    created_at: DateTime<Local>,
    hash: String,
}

impl ClipItem {
    fn from_text(text: String, hash: String) -> Self {
        let kind = classify_text(&text);

        Self {
            id: Uuid::new_v4(),
            kind,
            summary: summary_for_text(&text),
            text: Some(text),
            image: None,
            files: Vec::new(),
            created_at: Local::now(),
            hash,
        }
    }

    fn from_files(paths: Vec<PathBuf>, hash: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            kind: ClipKind::Files,
            summary: summary_for_files(&paths),
            text: None,
            image: None,
            files: paths,
            created_at: Local::now(),
            hash,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum ClipKind {
    Text,
    Url,
    FilePath,
    Email,
    Phone,
    Image,
    Files,
}

impl ClipKind {
    fn label(self) -> &'static str {
        match self {
            Self::Text => "Text",
            Self::Url => "URL",
            Self::FilePath => "File",
            Self::Email => "Email",
            Self::Phone => "Phone",
            Self::Image => "Image",
            Self::Files => "Files",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ImageClip {
    path: PathBuf,
    width: usize,
    height: usize,
}

struct BetterClipboardApp {
    store: Arc<Mutex<HistoryStore>>,
    shortcut: Option<ShortcutState>,
    _tray: Option<TrayState>,
    event_receiver: mpsc::Receiver<AppEvent>,
    settings: AppSettings,
    settings_open: bool,
    permission_onboarding: bool,
    permission_prompt_after: Option<Instant>,
    permission_prompt_requested: bool,
    selected: Option<Uuid>,
    status: String,
    textures: std::collections::HashMap<Uuid, TextureHandle>,
    window_visible: bool,
    force_quit: bool,
    select_first_on_show: bool,
    palette_expanded: bool,
    preview_item: Option<ImagePreviewState>,
    share_picker: Option<Retained<NSSharingServicePicker>>,
    previous_frontmost_pid: Option<i32>,
    shown_at: Instant,
    focus_loss_started: Option<Instant>,
    scroll_selected_into_view: bool,
    last_repaint: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ImagePreviewState {
    item_id: Uuid,
}

impl BetterClipboardApp {
    fn new(
        store: Arc<Mutex<HistoryStore>>,
        shortcut: Option<ShortcutState>,
        ctx: egui::Context,
        settings: AppSettings,
        permission_onboarding: bool,
    ) -> Self {
        apply_theme(&ctx, settings.theme);
        let event_receiver = setup_event_handlers(ctx);
        let (tray, status) = match setup_tray_icon() {
            Ok(tray) => (Some(tray), "Watching clipboard".to_owned()),
            Err(err) => (
                None,
                format!("Watching clipboard; tray icon unavailable: {err:#}"),
            ),
        };
        let status = if permission_onboarding {
            "Better Clipboard needs Accessibility permission for automatic paste.".to_owned()
        } else {
            status
        };

        Self {
            store,
            shortcut,
            _tray: tray,
            event_receiver,
            settings,
            settings_open: false,
            permission_onboarding,
            permission_prompt_after: None,
            permission_prompt_requested: false,
            selected: None,
            status,
            textures: std::collections::HashMap::new(),
            window_visible: permission_onboarding,
            force_quit: false,
            select_first_on_show: true,
            palette_expanded: false,
            preview_item: None,
            share_picker: None,
            previous_frontmost_pid: None,
            shown_at: Instant::now(),
            focus_loss_started: None,
            scroll_selected_into_view: false,
            last_repaint: Instant::now(),
        }
    }

    fn handle_app_events(&mut self, ctx: &egui::Context) {
        while let Ok(event) = self.event_receiver.try_recv() {
            match event {
                AppEvent::Hotkey(id) => {
                    if self
                        .shortcut
                        .as_ref()
                        .is_some_and(|shortcut| shortcut.hotkey_ids.contains(&id))
                    {
                        self.toggle_window(ctx);
                    }
                }
                AppEvent::TrayMenu(id) if id == TRAY_SHOW_ID => {
                    self.show_window(ctx);
                }
                AppEvent::TrayMenu(id) if id == TRAY_SETTINGS_ID => {
                    self.show_settings_window(ctx);
                }
                AppEvent::TrayMenu(id) if id == TRAY_QUIT_ID => {
                    self.force_quit = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                AppEvent::TrayMenu(_) => {}
            }
        }
    }

    fn show_window(&mut self, ctx: &egui::Context) {
        self.previous_frontmost_pid = frontmost_application_pid();
        self.window_visible = true;
        self.select_first_on_show = true;
        self.palette_expanded = false;
        self.settings_open = false;
        self.permission_onboarding = false;
        self.close_image_preview(ctx);
        self.scroll_selected_into_view = true;
        self.shown_at = Instant::now();
        self.focus_loss_started = None;
        self.resize_palette(ctx);
        if let Some(command) = egui::ViewportCommand::center_on_screen(ctx) {
            ctx.send_viewport_cmd(command);
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
            egui::WindowLevel::AlwaysOnTop,
        ));
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }

    fn show_settings_window(&mut self, ctx: &egui::Context) {
        self.show_window(ctx);
        self.settings_open = true;
        self.resize_palette(ctx);
    }

    fn toggle_window(&mut self, ctx: &egui::Context) {
        if self.window_visible {
            self.hide_window(ctx);
        } else {
            self.show_window(ctx);
        }
    }

    fn hide_window(&mut self, ctx: &egui::Context) {
        self.window_visible = false;
        self.settings_open = false;
        self.permission_onboarding = false;
        self.focus_loss_started = None;
        self.close_image_preview(ctx);
        self.close_share_picker();
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
    }

    fn resize_palette(&self, ctx: &egui::Context) {
        let height = if self.settings_open {
            SETTINGS_HEIGHT
        } else if self.palette_expanded {
            EXPANDED_HEIGHT
        } else {
            COMPACT_HEIGHT
        };
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(620.0, height)));
    }

    fn set_expanded(&mut self, expanded: bool, ctx: &egui::Context) {
        self.palette_expanded = expanded;
        self.settings_open = false;
        self.resize_palette(ctx);
    }

    fn save_settings(&mut self) {
        if let Err(err) = self.settings.save() {
            self.status = format!("Settings save failed: {err:#}");
        }
    }

    fn set_history_limit(&mut self, limit: usize) {
        let limit = limit.clamp(MIN_HISTORY_LIMIT, MAX_HISTORY_LIMIT);
        self.settings.history_limit = limit;
        if let Ok(mut store) = self.store.lock() {
            if let Err(err) = store.set_max_items(limit) {
                self.status = format!("History prune failed: {err:#}");
            }
        }
        self.save_settings();
    }

    fn set_launch_at_login(&mut self, enabled: bool) {
        match set_launch_agent_enabled(enabled) {
            Ok(()) => {
                self.settings.launch_at_login = enabled;
                self.status = if enabled {
                    "Run at login enabled".to_owned()
                } else {
                    "Run at login disabled".to_owned()
                };
                self.save_settings();
            }
            Err(err) => {
                self.status = format!("Run at login update failed: {err:#}");
                self.settings.launch_at_login = launch_agent_enabled();
            }
        }
    }

    fn copy_item(&mut self, item: &ClipItem, data_dir: &Path) -> bool {
        match copy_item_to_clipboard(item, data_dir) {
            Ok(()) => {
                let mut status = format!("Copied {}", item.kind.label().to_lowercase());
                if let Ok(mut store) = self.store.lock() {
                    if let Err(err) = store.promote(item.id) {
                        status = format!("Copied; history update failed: {err:#}");
                    }
                }
                self.status = status;
                self.selected = Some(item.id);
                true
            }
            Err(err) => {
                self.status = format!("Copy failed: {err:#}");
                false
            }
        }
    }

    fn copy_item_without_history_move(
        &mut self,
        item: &ClipItem,
        data_dir: &Path,
        status: &str,
    ) -> bool {
        if let Ok(mut store) = self.store.lock() {
            store.suppress_next_hash(&item.hash);
        }

        match copy_item_to_clipboard(item, data_dir) {
            Ok(()) => {
                self.status = status.to_owned();
                self.selected = Some(item.id);
                true
            }
            Err(err) => {
                if let Ok(mut store) = self.store.lock() {
                    store.allow_hash(&item.hash);
                }
                self.status = format!("Copy failed: {err:#}");
                false
            }
        }
    }

    fn copy_item_and_hide(
        &mut self,
        item: &ClipItem,
        data_dir: &Path,
        ctx: &egui::Context,
        status: &str,
    ) {
        if self.copy_item_without_history_move(item, data_dir, status) {
            self.close_image_preview(ctx);
            self.hide_window(ctx);
        }
    }

    fn copy_and_paste_item(&mut self, item: &ClipItem, data_dir: &Path, ctx: &egui::Context) {
        if self.copy_item(item, data_dir) {
            self.status = format!(
                "Copied {}; paste requested",
                item.kind.label().to_lowercase()
            );
            self.close_image_preview(ctx);
            self.hide_window(ctx);
            let target_pid = self.previous_frontmost_pid;
            thread::spawn(move || {
                thread::sleep(PASTE_DELAY);
                let _ = paste_current_clipboard(target_pid);
            });
        }
    }

    fn ensure_selection(&mut self, visible_items: &[ClipItem]) {
        if visible_items.is_empty() {
            self.selected = None;
            self.select_first_on_show = false;
            return;
        }

        let selected_is_visible = self
            .selected
            .is_some_and(|id| visible_items.iter().any(|item| item.id == id));

        if self.select_first_on_show || !selected_is_visible {
            self.selected = Some(visible_items[0].id);
            self.select_first_on_show = false;
            self.scroll_selected_into_view = true;
        }
    }

    fn selected_index(&self, visible_items: &[ClipItem]) -> Option<usize> {
        let selected = self.selected?;
        visible_items.iter().position(|item| item.id == selected)
    }

    fn move_selection(&mut self, visible_items: &[ClipItem], delta: isize) {
        if visible_items.is_empty() {
            self.selected = None;
            return;
        }

        let current = self.selected_index(visible_items).unwrap_or(0);
        let max = visible_items.len() as isize - 1;
        let next = (current as isize + delta).clamp(0, max) as usize;
        self.selected = Some(visible_items[next].id);
        self.scroll_selected_into_view = true;
    }

    fn close_image_preview(&mut self, ctx: &egui::Context) {
        if self.preview_item.take().is_some() {
            ctx.send_viewport_cmd_to(image_preview_viewport_id(), egui::ViewportCommand::Close);
        }
    }

    fn open_image_preview(&mut self, item: &ClipItem, ctx: &egui::Context) {
        if item.kind == ClipKind::Image {
            self.preview_item = Some(ImagePreviewState { item_id: item.id });
            ctx.request_repaint();
        }
    }

    fn close_share_picker(&mut self) {
        if let Some(picker) = self.share_picker.take() {
            picker.close();
        }
    }

    fn run_item_action(&mut self, item: &ClipItem, data_dir: &Path, ctx: &egui::Context) {
        match item.kind {
            ClipKind::Text => {
                self.copy_item(item, data_dir);
            }
            ClipKind::Url => {
                self.open_url(item, ctx);
            }
            ClipKind::FilePath => {
                self.reveal_file_in_finder(item, ctx);
            }
            ClipKind::Files => {
                self.reveal_file_in_finder(item, ctx);
            }
            ClipKind::Email => {
                self.open_email(item, ctx);
            }
            ClipKind::Phone => {
                self.open_phone(item, ctx);
            }
            ClipKind::Image => {
                self.open_image_preview(item, ctx);
            }
        }
    }

    fn open_item(&mut self, item: &ClipItem, data_dir: &Path, ctx: &egui::Context) {
        match item.kind {
            ClipKind::Text => {
                self.copy_item(item, data_dir);
            }
            ClipKind::Url => self.open_url(item, ctx),
            ClipKind::FilePath => self.open_file_path(item, ctx),
            ClipKind::Files => self.open_file_path(item, ctx),
            ClipKind::Email => self.open_email(item, ctx),
            ClipKind::Phone => self.open_phone(item, ctx),
            ClipKind::Image => self.open_image_preview(item, ctx),
        }
    }

    fn open_url(&mut self, item: &ClipItem, ctx: &egui::Context) {
        let Some(url) = item.text.as_deref().map(str::trim) else {
            self.status = "Open URL failed: missing URL payload".to_owned();
            return;
        };

        if let Err(err) = Url::parse(url) {
            self.status = format!("Open URL failed: {err}");
            return;
        }

        self.open_with_macos(url, "Opened URL", "Open URL failed", Some(item.id), ctx);
    }

    fn open_file_path(&mut self, item: &ClipItem, ctx: &egui::Context) {
        let paths = file_paths_for_item(item);
        if paths.is_empty() {
            self.status = "Open file failed: file path is unavailable".to_owned();
            return;
        }
        self.open_paths_with_macos(
            &paths,
            "Opened file",
            "Open file failed",
            Some(item.id),
            ctx,
        );
    }

    fn reveal_file_in_finder(&mut self, item: &ClipItem, ctx: &egui::Context) {
        let Some(path) = file_paths_for_item(item).into_iter().next() else {
            self.status = "Reveal failed: file path is unavailable".to_owned();
            return;
        };

        match Command::new("open").arg("-R").arg(path).spawn() {
            Ok(_) => {
                self.status = "Revealed in Finder".to_owned();
                self.selected = Some(item.id);
                self.hide_window(ctx);
            }
            Err(err) => {
                self.status = format!("Reveal failed: {err}");
            }
        }
    }

    fn open_email(&mut self, item: &ClipItem, ctx: &egui::Context) {
        let Some(email) = item.text.as_deref().map(str::trim) else {
            self.status = "Open email failed: missing email payload".to_owned();
            return;
        };
        if !is_email_address(email) {
            self.status = "Open email failed: invalid email address".to_owned();
            return;
        }
        let target = format!("mailto:{email}");
        self.open_with_macos(
            &target,
            "Opened email composer",
            "Open email failed",
            Some(item.id),
            ctx,
        );
    }

    fn open_phone(&mut self, item: &ClipItem, ctx: &egui::Context) {
        let Some(phone) = item.text.as_deref().and_then(phone_url) else {
            self.status = "Open phone failed: invalid phone number".to_owned();
            return;
        };
        self.open_with_macos(
            &phone,
            "Opened phone handler",
            "Open phone failed",
            Some(item.id),
            ctx,
        );
    }

    fn run_row_action(
        &mut self,
        action: RowAction,
        item: &ClipItem,
        data_dir: &Path,
        ctx: &egui::Context,
    ) {
        match action {
            RowAction::Paste => self.copy_and_paste_item(item, data_dir, ctx),
            RowAction::Copy => self.copy_item_and_hide(
                item,
                data_dir,
                ctx,
                &format!("Copied {}", item.kind.label().to_lowercase()),
            ),
            RowAction::Open => self.open_item(item, data_dir, ctx),
            RowAction::Reveal => self.reveal_file_in_finder(item, ctx),
            RowAction::Preview => self.open_image_preview(item, ctx),
            RowAction::Share => self.share_item(item, data_dir),
        }
    }

    fn open_with_macos(
        &mut self,
        target: &str,
        success: &str,
        failure: &str,
        selected: Option<Uuid>,
        ctx: &egui::Context,
    ) {
        match Command::new("open").arg(target).spawn() {
            Ok(_) => {
                self.status = success.to_owned();
                self.selected = selected;
                self.hide_window(ctx);
            }
            Err(err) => {
                self.status = format!("{failure}: {err}");
            }
        }
    }

    fn open_paths_with_macos(
        &mut self,
        paths: &[PathBuf],
        success: &str,
        failure: &str,
        selected: Option<Uuid>,
        ctx: &egui::Context,
    ) {
        match Command::new("open").args(paths).spawn() {
            Ok(_) => {
                self.status = success.to_owned();
                self.selected = selected;
                self.hide_window(ctx);
            }
            Err(err) => {
                self.status = format!("{failure}: {err}");
            }
        }
    }

    fn share_item(&mut self, item: &ClipItem, data_dir: &Path) {
        match open_share_sheet(item, data_dir) {
            Ok(picker) => {
                self.share_picker = Some(picker);
                self.status = format!("Sharing {}", item.kind.label().to_lowercase());
                self.selected = Some(item.id);
                self.focus_loss_started = None;
            }
            Err(err) => {
                self.status = format!("Share failed: {err:#}");
            }
        }
    }

    fn handle_focus_loss(&mut self, ctx: &egui::Context) {
        if !self.window_visible || self.permission_onboarding {
            self.focus_loss_started = None;
            return;
        }

        if self.shown_at.elapsed() < FOCUS_HIDE_GRACE {
            return;
        }

        if better_clipboard_has_focus(ctx) {
            self.focus_loss_started = None;
            return;
        }

        let started = self.focus_loss_started.get_or_insert_with(Instant::now);
        if started.elapsed() >= FOCUS_HIDE_GRACE {
            self.hide_window(ctx);
        } else {
            ctx.request_repaint_after(FOCUS_HIDE_GRACE);
        }
    }

    fn handle_keyboard(
        &mut self,
        ctx: &egui::Context,
        visible_items: &[ClipItem],
        data_dir: &Path,
    ) {
        let down = ctx.input(|input| input.key_pressed(Key::ArrowDown));
        let up = ctx.input(|input| input.key_pressed(Key::ArrowUp));
        let command_down =
            ctx.input(|input| input.modifiers.command && input.key_pressed(Key::ArrowDown));
        let command_up =
            ctx.input(|input| input.modifiers.command && input.key_pressed(Key::ArrowUp));
        let tab = ctx.input(|input| input.key_pressed(Key::Tab));
        let enter = ctx.input(|input| input.key_pressed(Key::Enter));
        let escape = ctx.input(|input| input.key_pressed(Key::Escape));
        let right = ctx.input(|input| input.key_pressed(Key::ArrowRight));
        let left = ctx.input(|input| input.key_pressed(Key::ArrowLeft));
        let open = ctx.input(|input| input.key_pressed(Key::O));
        let copy = ctx.input(|input| input.key_pressed(Key::C));
        let finder = ctx.input(|input| input.key_pressed(Key::F));
        let share = ctx.input(|input| input.key_pressed(Key::S));

        if self.preview_item.is_some() {
            if right {
                return;
            }
            if left || escape {
                self.close_image_preview(ctx);
                return;
            }
            if enter {
                if let Some(index) = self.selected_index(visible_items) {
                    self.copy_and_paste_item(&visible_items[index], data_dir, ctx);
                }
            }
            if copy {
                if let Some(index) = self.selected_index(visible_items) {
                    let item = &visible_items[index];
                    self.copy_item_and_hide(
                        item,
                        data_dir,
                        ctx,
                        &format!("Copied {}", item.kind.label().to_lowercase()),
                    );
                }
            }
            if share {
                if let Some(index) = self.selected_index(visible_items) {
                    let item = &visible_items[index];
                    self.close_image_preview(ctx);
                    self.share_item(item, data_dir);
                }
            }
            return;
        }

        if escape {
            self.hide_window(ctx);
            return;
        }

        if self.settings_open {
            return;
        }

        if command_down || tab {
            self.set_expanded(true, ctx);
            return;
        }

        if command_up {
            self.set_expanded(false, ctx);
            return;
        }

        if down {
            self.set_expanded(true, ctx);
            self.move_selection(visible_items, 1);
        }
        if up {
            self.set_expanded(true, ctx);
            self.move_selection(visible_items, -1);
        }
        if right {
            if let Some(index) = self.selected_index(visible_items) {
                self.open_image_preview(&visible_items[index], ctx);
            }
        }
        if open {
            if let Some(index) = self.selected_index(visible_items) {
                self.open_item(&visible_items[index], data_dir, ctx);
            }
        }
        if copy {
            if let Some(index) = self.selected_index(visible_items) {
                let item = &visible_items[index];
                self.copy_item_and_hide(
                    item,
                    data_dir,
                    ctx,
                    &format!("Copied {}", item.kind.label().to_lowercase()),
                );
            }
        }
        if finder {
            if let Some(index) = self.selected_index(visible_items) {
                if matches!(
                    visible_items[index].kind,
                    ClipKind::FilePath | ClipKind::Files
                ) {
                    self.reveal_file_in_finder(&visible_items[index], ctx);
                }
            }
        }
        if share {
            if let Some(index) = self.selected_index(visible_items) {
                self.share_item(&visible_items[index], data_dir);
            }
        }
        if enter {
            if let Some(index) = self.selected_index(visible_items) {
                self.copy_and_paste_item(&visible_items[index], data_dir, ctx);
            }
        }
    }

    fn handle_scroll_intent(&mut self, ctx: &egui::Context, item_count: usize) {
        if !self.window_visible || self.settings_open || self.permission_onboarding {
            return;
        }

        let scrolled = ctx.input(|input| {
            input.raw_scroll_delta.y.abs() > 0.0 || input.smooth_scroll_delta.y.abs() > 0.0
        });
        if !scrolled {
            return;
        }

        self.scroll_selected_into_view = false;
        if item_count > COMPACT_VISIBLE_ITEMS && !self.palette_expanded {
            self.set_expanded(true, ctx);
        }
    }

    fn show_settings(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.vertical(|ui| {
            ui.label(RichText::new("Settings").strong().size(16.0));
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                ui.label("Theme");
                let mut theme = self.settings.theme;
                for option in [ThemeMode::Light, ThemeMode::Dark] {
                    ui.selectable_value(&mut theme, option, option.label());
                }
                if theme != self.settings.theme {
                    self.settings.theme = theme;
                    apply_theme(ctx, theme);
                    self.save_settings();
                }
            });

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label("History limit");
                let mut limit = self.settings.history_limit;
                let response = ui.add(
                    egui::DragValue::new(&mut limit)
                        .range(MIN_HISTORY_LIMIT..=MAX_HISTORY_LIMIT)
                        .speed(1),
                );
                if response.changed() {
                    self.set_history_limit(limit);
                }
            });

            ui.add_space(8.0);
            let mut launch_at_login = self.settings.launch_at_login;
            if ui
                .checkbox(&mut launch_at_login, "Run at login")
                .on_hover_text("Start Better Clipboard when you log in to macOS")
                .changed()
            {
                self.set_launch_at_login(launch_at_login);
            }

            ui.add_space(8.0);
            let accessibility_status = if accessibility_permission_granted() {
                "Accessibility permission granted"
            } else {
                "Accessibility permission needed for automatic paste"
            };
            ui.label(RichText::new(accessibility_status).color(muted_text(self.settings.theme)));
            if !accessibility_permission_granted()
                && ui.button("Request Accessibility Permission").clicked()
            {
                if request_accessibility_permission() {
                    self.status = "Accessibility permission granted".to_owned();
                } else {
                    self.status = "Accessibility permission requested".to_owned();
                }
            }
        });
    }

    fn show_permission_onboarding(&mut self, ctx: &egui::Context) {
        if accessibility_permission_granted() {
            self.permission_onboarding = false;
            self.permission_prompt_after = None;
            self.hide_window(ctx);
            return;
        }

        if self.permission_prompt_after.is_none() && !self.permission_prompt_requested {
            self.permission_prompt_after = Some(Instant::now() + PERMISSION_PROMPT_DELAY);
            self.status =
                "Better Clipboard will ask macOS for Accessibility permission.".to_owned();
            ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
                egui::WindowLevel::Normal,
            ));
            ctx.request_repaint_after(PERMISSION_PROMPT_DELAY);
        }

        egui::CentralPanel::default()
            .frame(
                egui::Frame::NONE
                    .fill(palette_background(self.settings.theme))
                    .inner_margin(18),
            )
            .show(ctx, |ui| {
                ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.label(RichText::new(APP_NAME).strong().size(18.0));
                    ui.add_space(10.0);
                    ui.label("Accessibility permission is needed to paste into the app you were using after choosing a clipboard item.");
                    ui.add_space(6.0);
                    ui.label("Better Clipboard will open the macOS permission prompt after this window is visible.");
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if ui.button("Request Now").clicked() {
                            self.permission_prompt_after = None;
                            self.permission_prompt_requested = true;
                            if request_accessibility_permission() {
                                self.permission_onboarding = false;
                                self.hide_window(ctx);
                            } else {
                                self.status =
                                    "Accessibility permission requested in System Settings"
                                        .to_owned();
                            }
                        }
                        if ui.button("Check Again").clicked() && accessibility_permission_granted()
                        {
                            self.permission_onboarding = false;
                            self.hide_window(ctx);
                        }
                        if ui.button("Not Now").clicked() {
                            self.hide_window(ctx);
                        }
                    });
                    ui.add_space(10.0);
                    ui.label(RichText::new(&self.status).color(muted_text(self.settings.theme)));
                });
            });

        if self
            .permission_prompt_after
            .is_some_and(|prompt_at| Instant::now() >= prompt_at)
        {
            self.permission_prompt_after = None;
            self.permission_prompt_requested = true;
            self.status = "Opening macOS Accessibility permission prompt.".to_owned();
            if request_accessibility_permission() {
                self.permission_onboarding = false;
                self.hide_window(ctx);
            } else {
                self.status =
                    "Permission requested. Allow Better Clipboard in System Settings, then click Check Again."
                        .to_owned();
                ctx.request_repaint_after(Duration::from_millis(500));
            }
        }
    }
}

impl eframe::App for BetterClipboardApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_app_events(ctx);

        if ctx.input(|input| input.viewport().close_requested()) && !self.force_quit {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.hide_window(ctx);
            return;
        }

        if self.last_repaint.elapsed() > Duration::from_millis(500) {
            ctx.request_repaint();
            self.last_repaint = Instant::now();
        }

        if self.permission_onboarding {
            self.show_permission_onboarding(ctx);
            return;
        }

        let (data_dir, items) = match self.store.lock() {
            Ok(store) => (store.data_dir.clone(), store.items.clone()),
            Err(_) => {
                self.status = "History is temporarily unavailable".to_owned();
                return;
            }
        };

        let visible_items = items.clone();
        self.ensure_selection(&visible_items);
        self.handle_keyboard(ctx, &visible_items, &data_dir);
        self.handle_scroll_intent(ctx, visible_items.len());
        let display_items: Vec<ClipItem> = if self.settings_open {
            Vec::new()
        } else if self.palette_expanded {
            visible_items.clone()
        } else {
            visible_items
                .iter()
                .take(COMPACT_VISIBLE_ITEMS)
                .cloned()
                .collect()
        };
        let bg = palette_background(self.settings.theme);
        let row_bg = row_background(self.settings.theme);
        let selected_bg = selected_row_background(self.settings.theme);
        let muted = muted_text(self.settings.theme);
        let shown_count = display_items.len();

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(bg).inner_margin(14))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("📋").size(22.0));
                    ui.label(RichText::new(APP_NAME).strong().size(18.0));
                    ui.separator();
                    let count_text = if self.palette_expanded {
                        format!("{} items", visible_items.len())
                    } else if visible_items.is_empty() {
                        "0 items".to_owned()
                    } else {
                        format!("Showing {} of {}", shown_count, visible_items.len())
                    };
                    ui.label(RichText::new(count_text).color(muted));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("×").on_hover_text("Close").clicked() {
                            self.hide_window(ctx);
                        }
                        if ui.button("⚙").on_hover_text("Settings").clicked() {
                            self.settings_open = !self.settings_open;
                            self.resize_palette(ctx);
                        }
                        let expand_label = if self.palette_expanded { "⌃" } else { "⌄" };
                        if ui
                            .button(expand_label)
                            .on_hover_text(if self.palette_expanded {
                                "Collapse"
                            } else {
                                "Expand"
                            })
                            .clicked()
                        {
                            self.set_expanded(!self.palette_expanded, ctx);
                        }
                    });
                });

                ui.add_space(8.0);
                if self.settings_open {
                    self.show_settings(ui, ctx);
                    return;
                }

                if visible_items.is_empty() {
                    ui.centered_and_justified(|ui| {
                        ui.label("Copy text, a URL, or an image to start building history.");
                    });
                    return;
                }

                let mut did_scroll_selected = false;
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for item in display_items {
                        let selected = self.selected == Some(item.id);
                        let fill = if selected { selected_bg } else { row_bg };
                        let row_width = ui.available_width();
                        let mut row_action = None;
                        let row_size = egui::vec2(row_width, ROW_HEIGHT + 20.0);
                        let (row_rect, response) =
                            ui.allocate_exact_size(row_size, egui::Sense::click());
                        ui.painter()
                            .rect_filled(row_rect, CornerRadius::same(3), fill);
                        ui.painter().rect_stroke(
                            row_rect,
                            CornerRadius::same(3),
                            Stroke::new(1.0, hint_chip_stroke(self.settings.theme)),
                            egui::StrokeKind::Inside,
                        );

                        let inner_rect = row_rect.shrink(10.0);
                        ui.scope_builder(egui::UiBuilder::new().max_rect(inner_rect), |ui| {
                            ui.set_min_size(inner_rect.size());
                            ui.horizontal_top(|ui| {
                                let action_response = show_item_action_button(
                                    ui,
                                    ctx,
                                    &mut self.textures,
                                    &data_dir,
                                    &item,
                                    self.settings.theme,
                                );
                                if action_response.clicked() {
                                    self.run_item_action(&item, &data_dir, ctx);
                                    ctx.request_repaint();
                                }
                                ui.add_space(8.0);
                                let content_width = ui.available_width();
                                ui.allocate_ui_with_layout(
                                    egui::vec2(content_width, ROW_HEIGHT),
                                    egui::Layout::top_down(egui::Align::Min),
                                    |ui| {
                                        ui.spacing_mut().item_spacing.y = 2.0;
                                        let text_color = if selected {
                                            selected_text(self.settings.theme)
                                        } else {
                                            ui.visuals().text_color()
                                        };
                                        ui.add_sized(
                                            egui::vec2(content_width, TEXT_PREVIEW_HEIGHT),
                                            egui::Label::new(
                                                RichText::new(&item.summary).color(text_color),
                                            )
                                            .wrap()
                                            .halign(egui::Align::Min),
                                        );
                                        ui.allocate_ui_with_layout(
                                            egui::vec2(content_width, HINT_CHIP_HEIGHT),
                                            egui::Layout::left_to_right(egui::Align::Center),
                                            |ui| {
                                                let actions_width =
                                                    row_action_buttons_width(item.kind);
                                                let metadata_width =
                                                    (content_width - actions_width - 10.0).max(0.0);
                                                ui.add_sized(
                                                    egui::vec2(metadata_width, HINT_CHIP_HEIGHT),
                                                    egui::Label::new(
                                                        RichText::new(format!(
                                                            "{} · {}",
                                                            item.kind.label(),
                                                            item.created_at.format("%H:%M:%S")
                                                        ))
                                                        .color(muted),
                                                    )
                                                    .truncate()
                                                    .halign(egui::Align::Min),
                                                );
                                                ui.with_layout(
                                                    egui::Layout::right_to_left(
                                                        egui::Align::Center,
                                                    ),
                                                    |ui| {
                                                        row_action = show_row_action_buttons(
                                                            ui,
                                                            item.kind,
                                                            self.settings.theme,
                                                        );
                                                    },
                                                );
                                            },
                                        );
                                    },
                                );
                            });
                        });

                        if let Some(action) = row_action {
                            self.selected = Some(item.id);
                            self.run_row_action(action, &item, &data_dir, ctx);
                            ctx.request_repaint();
                        } else {
                            if response.clicked() {
                                self.selected = Some(item.id);
                            }

                            if response.double_clicked() {
                                self.copy_and_paste_item(&item, &data_dir, ctx);
                            }
                        }

                        if selected && self.scroll_selected_into_view {
                            response.scroll_to_me(Some(egui::Align::Center));
                            did_scroll_selected = true;
                        }
                        ui.add_space(6.0);
                    }
                });
                if did_scroll_selected {
                    self.scroll_selected_into_view = false;
                }
            });

        self.show_image_preview(ctx, &data_dir, &visible_items);
        self.handle_focus_loss(ctx);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PreviewAction {
    None,
    Close,
    Paste,
}

impl BetterClipboardApp {
    fn show_image_preview(&mut self, ctx: &egui::Context, data_dir: &Path, items: &[ClipItem]) {
        let Some(preview) = self.preview_item else {
            return;
        };
        let Some(item) = items
            .iter()
            .find(|item| item.id == preview.item_id)
            .cloned()
        else {
            self.close_image_preview(ctx);
            return;
        };
        let Some(image) = item.image.clone() else {
            self.close_image_preview(ctx);
            return;
        };

        let image_size = preview_image_size(ctx, image.width, image.height);
        let window_size = image_size;

        if !self.textures.contains_key(&item.id) {
            if let Ok(color_image) = load_color_image(&data_dir.join(&image.path)) {
                let texture = ctx.load_texture(
                    item.id.to_string(),
                    color_image,
                    egui::TextureOptions::LINEAR,
                );
                self.textures.insert(item.id, texture);
            }
        }
        let texture = self.textures.get(&item.id).cloned();
        let theme = self.settings.theme;
        let builder = image_preview_viewport_builder(ctx, window_size);

        let action = ctx.show_viewport_immediate(image_preview_viewport_id(), builder, |ctx, _| {
            apply_theme(ctx, theme);

            if ctx.input(|input| input.viewport().close_requested()) {
                return PreviewAction::Close;
            }
            if ctx.input(|input| input.key_pressed(Key::Escape)) {
                return PreviewAction::Close;
            }
            if ctx.input(|input| input.key_pressed(Key::ArrowLeft)) {
                return PreviewAction::Close;
            }
            if ctx.input(|input| input.key_pressed(Key::Enter)) {
                return PreviewAction::Paste;
            }

            egui::CentralPanel::default()
                .frame(egui::Frame::NONE.fill(Color32::TRANSPARENT).inner_margin(0))
                .show(ctx, |ui| {
                    if let Some(texture) = &texture {
                        let (rect, _) = ui.allocate_exact_size(image_size, egui::Sense::hover());
                        ui.painter().image(
                            texture.id(),
                            rect,
                            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                            Color32::WHITE,
                        );
                    } else {
                        ui.centered_and_justified(|ui| {
                            ui.label("Preview unavailable");
                        });
                    }
                });

            PreviewAction::None
        });

        match action {
            PreviewAction::None => {}
            PreviewAction::Close => self.close_image_preview(ctx),
            PreviewAction::Paste => self.copy_and_paste_item(&item, data_dir, ctx),
        }
    }
}

fn palette_background(theme: ThemeMode) -> Color32 {
    match theme {
        ThemeMode::Light => Color32::from_rgb(246, 246, 244),
        ThemeMode::Dark => Color32::from_rgb(18, 18, 18),
    }
}

fn image_preview_viewport_id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("better-clipboard-image-preview")
}

fn image_preview_viewport_builder(ctx: &egui::Context, size: egui::Vec2) -> egui::ViewportBuilder {
    let mut builder = egui::ViewportBuilder::default()
        .with_inner_size(size)
        .with_min_inner_size(size)
        .with_max_inner_size(size)
        .with_resizable(false)
        .with_decorations(false)
        .with_transparent(true)
        .with_always_on_top()
        .with_active(true)
        .with_title("Better Clipboard Image Preview");

    if let Some(position) = image_preview_position(ctx, size) {
        builder = builder.with_position(position);
    }

    builder
}

fn image_preview_position(ctx: &egui::Context, size: egui::Vec2) -> Option<egui::Pos2> {
    const SCREEN_MARGIN: f32 = 8.0;

    ctx.input(|input| {
        let monitor_size = input.viewport().monitor_size?;
        let x = ((monitor_size.x - size.x) * 0.5).max(SCREEN_MARGIN);
        let y = ((monitor_size.y - size.y) * 0.5).max(SCREEN_MARGIN);
        Some(egui::pos2(x, y))
    })
}

fn preview_image_size(ctx: &egui::Context, width: usize, height: usize) -> egui::Vec2 {
    let width = width.max(1) as f32;
    let height = height.max(1) as f32;
    let pixels_per_point = ctx.pixels_per_point().max(1.0);
    let native_size = egui::vec2(width / pixels_per_point, height / pixels_per_point);
    let scaled = native_size * PREVIEW_SCALE;
    let cap_scale = (PREVIEW_MAX_IMAGE_WIDTH / scaled.x)
        .min(PREVIEW_MAX_IMAGE_HEIGHT / scaled.y)
        .min(1.0);
    scaled * cap_scale
}

fn better_clipboard_has_focus(ctx: &egui::Context) -> bool {
    ctx.input(|input| {
        let root_focused = input.viewport().focused.unwrap_or(input.focused);
        let preview_focused = input
            .raw
            .viewports
            .get(&image_preview_viewport_id())
            .and_then(|viewport| viewport.focused)
            .unwrap_or(false);
        root_focused || preview_focused
    })
}

fn row_background(theme: ThemeMode) -> Color32 {
    match theme {
        ThemeMode::Light => Color32::from_rgb(255, 255, 255),
        ThemeMode::Dark => Color32::from_rgb(28, 28, 28),
    }
}

fn selected_row_background(theme: ThemeMode) -> Color32 {
    match theme {
        ThemeMode::Light => Color32::from_rgb(224, 237, 255),
        ThemeMode::Dark => Color32::from_rgb(38, 57, 72),
    }
}

fn selected_text(theme: ThemeMode) -> Color32 {
    match theme {
        ThemeMode::Light => Color32::from_rgb(26, 60, 110),
        ThemeMode::Dark => Color32::from_rgb(170, 210, 255),
    }
}

fn muted_text(theme: ThemeMode) -> Color32 {
    match theme {
        ThemeMode::Light => Color32::from_rgb(95, 95, 95),
        ThemeMode::Dark => Color32::from_rgb(155, 155, 155),
    }
}

fn thumbnail_background(theme: ThemeMode) -> Color32 {
    match theme {
        ThemeMode::Light => Color32::from_rgb(234, 234, 232),
        ThemeMode::Dark => Color32::from_rgb(46, 46, 46),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RowAction {
    Paste,
    Copy,
    Open,
    Reveal,
    Preview,
    Share,
}

fn show_row_action_buttons(
    ui: &mut egui::Ui,
    kind: ClipKind,
    theme: ThemeMode,
) -> Option<RowAction> {
    ui.spacing_mut().item_spacing.x = HINT_CHIP_GAP;
    let mut action = None;

    if show_row_action_button(
        ui,
        ActionIcon::Share,
        "Share",
        "S",
        "Open macOS share sheet",
        theme,
    )
    .clicked()
    {
        action = Some(RowAction::Share);
    }

    match kind {
        ClipKind::Text => {
            if show_row_action_button(
                ui,
                ActionIcon::Copy,
                "Copy",
                "C",
                "Copy without pasting",
                theme,
            )
            .clicked()
            {
                action = Some(RowAction::Copy);
            }
        }
        ClipKind::Url => {
            if show_row_action_button(ui, ActionIcon::Open, "Open", "O", "Open URL", theme)
                .clicked()
            {
                action = Some(RowAction::Open);
            }
        }
        ClipKind::FilePath => {
            if show_row_action_button(
                ui,
                ActionIcon::Finder,
                "Finder",
                "F",
                "Reveal in Finder",
                theme,
            )
            .clicked()
            {
                action = Some(RowAction::Reveal);
            }
            if show_row_action_button(ui, ActionIcon::Open, "Open", "O", "Open file", theme)
                .clicked()
            {
                action = Some(RowAction::Open);
            }
        }
        ClipKind::Files => {
            if show_row_action_button(
                ui,
                ActionIcon::Finder,
                "Finder",
                "F",
                "Reveal in Finder",
                theme,
            )
            .clicked()
            {
                action = Some(RowAction::Reveal);
            }
            if show_row_action_button(ui, ActionIcon::Open, "Open", "O", "Open files", theme)
                .clicked()
            {
                action = Some(RowAction::Open);
            }
        }
        ClipKind::Email => {
            if show_row_action_button(ui, ActionIcon::Email, "Email", "O", "Compose email", theme)
                .clicked()
            {
                action = Some(RowAction::Open);
            }
        }
        ClipKind::Phone => {
            if show_row_action_button(
                ui,
                ActionIcon::Phone,
                "Call",
                "O",
                "Open phone handler",
                theme,
            )
            .clicked()
            {
                action = Some(RowAction::Open);
            }
        }
        ClipKind::Image => {
            if show_row_action_button(
                ui,
                ActionIcon::Preview,
                "Preview",
                "Right",
                "Preview image",
                theme,
            )
            .clicked()
            {
                action = Some(RowAction::Preview);
            }
        }
    }

    if show_row_action_button(
        ui,
        ActionIcon::Paste,
        "Paste",
        "Enter",
        "Paste into the previous app",
        theme,
    )
    .clicked()
    {
        action = Some(RowAction::Paste);
    }

    action
}

fn row_action_buttons_width(kind: ClipKind) -> f32 {
    let count = row_action_button_count(kind) as f32;
    (count * HINT_CHIP_WIDTH) + ((count - 1.0).max(0.0) * HINT_CHIP_GAP)
}

fn row_action_button_count(kind: ClipKind) -> usize {
    let paste = 1;
    let share = 1;
    let type_specific = match kind {
        ClipKind::Text | ClipKind::Url | ClipKind::Email | ClipKind::Phone | ClipKind::Image => 1,
        ClipKind::FilePath | ClipKind::Files => 2,
    };
    paste + share + type_specific
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActionIcon {
    Copy,
    Open,
    Finder,
    Email,
    Phone,
    Preview,
    Paste,
    Share,
}

fn show_row_action_button(
    ui: &mut egui::Ui,
    icon: ActionIcon,
    label: &str,
    shortcut: &str,
    hover_text: &str,
    theme: ThemeMode,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(HINT_CHIP_WIDTH, HINT_CHIP_HEIGHT),
        egui::Sense::click(),
    );
    let fill = if response.hovered() {
        hint_chip_hover_background(theme)
    } else {
        hint_chip_background(theme)
    };
    ui.painter().rect_filled(rect, CornerRadius::same(5), fill);
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(5),
        Stroke::new(1.0, hint_chip_stroke(theme)),
        egui::StrokeKind::Inside,
    );
    let icon_rect = egui::Rect::from_center_size(
        egui::pos2(rect.center().x, rect.top() + 12.0),
        egui::vec2(14.0, 14.0),
    );
    draw_action_icon(ui.painter(), icon_rect, icon, muted_text(theme), 1.4);
    ui.painter().text(
        egui::pos2(rect.center().x, rect.top() + 28.0),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(10.0),
        muted_text(theme),
    );
    ui.painter().text(
        egui::pos2(rect.center().x, rect.bottom() - 7.0),
        egui::Align2::CENTER_CENTER,
        shortcut,
        egui::FontId::proportional(8.5),
        muted_text(theme),
    );
    response.on_hover_text(hover_text)
}

fn hint_chip_background(theme: ThemeMode) -> Color32 {
    match theme {
        ThemeMode::Light => Color32::from_rgb(248, 248, 246),
        ThemeMode::Dark => Color32::from_rgb(30, 30, 30),
    }
}

fn hint_chip_hover_background(theme: ThemeMode) -> Color32 {
    match theme {
        ThemeMode::Light => Color32::from_rgb(238, 238, 235),
        ThemeMode::Dark => Color32::from_rgb(42, 42, 42),
    }
}

fn hint_chip_stroke(theme: ThemeMode) -> Color32 {
    match theme {
        ThemeMode::Light => Color32::from_rgb(205, 205, 202),
        ThemeMode::Dark => Color32::from_rgb(70, 70, 70),
    }
}

fn draw_action_icon(
    painter: &egui::Painter,
    rect: egui::Rect,
    icon: ActionIcon,
    color: Color32,
    stroke_width: f32,
) {
    let stroke = Stroke::new(stroke_width, color);
    match icon {
        ActionIcon::Copy => {
            let back = egui::Rect::from_min_size(
                rect.left_top() + egui::vec2(rect.width() * 0.24, 0.0),
                rect.size() * 0.62,
            );
            let front = egui::Rect::from_min_size(
                rect.left_top() + egui::vec2(0.0, rect.height() * 0.22),
                rect.size() * 0.68,
            );
            painter.rect_stroke(
                back,
                CornerRadius::same(2),
                stroke,
                egui::StrokeKind::Inside,
            );
            painter.rect_stroke(
                front,
                CornerRadius::same(2),
                stroke,
                egui::StrokeKind::Inside,
            );
        }
        ActionIcon::Open => {
            let start = egui::pos2(
                rect.left() + rect.width() * 0.22,
                rect.bottom() - rect.height() * 0.22,
            );
            let end = egui::pos2(
                rect.right() - rect.width() * 0.18,
                rect.top() + rect.height() * 0.18,
            );
            painter.line_segment([start, end], stroke);
            painter.line_segment(
                [
                    end,
                    egui::pos2(end.x - rect.width() * 0.38, end.y + rect.height() * 0.02),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    end,
                    egui::pos2(end.x - rect.width() * 0.02, end.y + rect.height() * 0.38),
                ],
                stroke,
            );
        }
        ActionIcon::Finder => {
            let tab_left = egui::pos2(rect.left(), rect.top() + rect.height() * 0.34);
            let tab_top = egui::pos2(
                rect.left() + rect.width() * 0.28,
                rect.top() + rect.height() * 0.18,
            );
            let tab_right = egui::pos2(
                rect.left() + rect.width() * 0.48,
                rect.top() + rect.height() * 0.34,
            );
            let points = [
                tab_left,
                tab_top,
                tab_right,
                egui::pos2(rect.right(), rect.top() + rect.height() * 0.34),
                rect.right_bottom(),
                rect.left_bottom(),
                tab_left,
            ];
            for pair in points.windows(2) {
                painter.line_segment([pair[0], pair[1]], stroke);
            }
        }
        ActionIcon::Email => {
            painter.rect_stroke(
                rect,
                CornerRadius::same(2),
                stroke,
                egui::StrokeKind::Inside,
            );
            painter.line_segment([rect.left_top(), rect.center()], stroke);
            painter.line_segment([rect.right_top(), rect.center()], stroke);
            painter.line_segment([rect.left_bottom(), rect.center()], stroke);
            painter.line_segment([rect.right_bottom(), rect.center()], stroke);
        }
        ActionIcon::Phone => {
            let points = [
                egui::pos2(
                    rect.left() + rect.width() * 0.18,
                    rect.top() + rect.height() * 0.28,
                ),
                egui::pos2(
                    rect.left() + rect.width() * 0.34,
                    rect.top() + rect.height() * 0.18,
                ),
                egui::pos2(
                    rect.left() + rect.width() * 0.48,
                    rect.top() + rect.height() * 0.40,
                ),
                egui::pos2(
                    rect.left() + rect.width() * 0.60,
                    rect.top() + rect.height() * 0.54,
                ),
                egui::pos2(
                    rect.left() + rect.width() * 0.82,
                    rect.top() + rect.height() * 0.66,
                ),
                egui::pos2(
                    rect.left() + rect.width() * 0.70,
                    rect.top() + rect.height() * 0.84,
                ),
            ];
            for pair in points.windows(2) {
                painter.line_segment([pair[0], pair[1]], stroke);
            }
        }
        ActionIcon::Preview => {
            let radius = rect.width().min(rect.height()) * 0.28;
            let center = rect.center() - egui::vec2(rect.width() * 0.08, rect.height() * 0.08);
            painter.circle_stroke(center, radius, stroke);
            painter.line_segment(
                [
                    center + egui::vec2(radius * 0.72, radius * 0.72),
                    rect.right_bottom(),
                ],
                stroke,
            );
        }
        ActionIcon::Paste => {
            let top = egui::pos2(
                rect.right() - rect.width() * 0.16,
                rect.top() + rect.height() * 0.18,
            );
            let mid = egui::pos2(top.x, rect.center().y);
            let left = egui::pos2(rect.left() + rect.width() * 0.22, mid.y);
            painter.line_segment([top, mid], stroke);
            painter.line_segment([mid, left], stroke);
            painter.line_segment(
                [
                    left,
                    left + egui::vec2(rect.width() * 0.22, -rect.height() * 0.18),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    left,
                    left + egui::vec2(rect.width() * 0.22, rect.height() * 0.18),
                ],
                stroke,
            );
        }
        ActionIcon::Share => {
            let box_top = rect.top() + rect.height() * 0.50;
            let box_rect = egui::Rect::from_min_max(
                egui::pos2(rect.left() + rect.width() * 0.16, box_top),
                egui::pos2(
                    rect.right() - rect.width() * 0.16,
                    rect.bottom() - rect.height() * 0.08,
                ),
            );
            painter.line_segment([box_rect.left_top(), box_rect.left_bottom()], stroke);
            painter.line_segment([box_rect.left_bottom(), box_rect.right_bottom()], stroke);
            painter.line_segment([box_rect.right_bottom(), box_rect.right_top()], stroke);

            let arrow_top = egui::pos2(rect.center().x, rect.top() + rect.height() * 0.08);
            let arrow_bottom = egui::pos2(rect.center().x, rect.bottom() - rect.height() * 0.28);
            painter.line_segment([arrow_bottom, arrow_top], stroke);
            painter.line_segment(
                [
                    arrow_top,
                    arrow_top + egui::vec2(-rect.width() * 0.18, rect.height() * 0.20),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    arrow_top,
                    arrow_top + egui::vec2(rect.width() * 0.18, rect.height() * 0.20),
                ],
                stroke,
            );
        }
    }
}

fn show_item_action_button(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    textures: &mut std::collections::HashMap<Uuid, TextureHandle>,
    data_dir: &Path,
    item: &ClipItem,
    theme: ThemeMode,
) -> egui::Response {
    let size = action_tile_size(item);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(7), thumbnail_background(theme));
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(7),
        Stroke::new(1.0, muted_text(theme)),
        egui::StrokeKind::Inside,
    );

    if let Some(image) = &item.image {
        if !textures.contains_key(&item.id) {
            if let Ok(color_image) = load_color_image(&data_dir.join(&image.path)) {
                let texture = ctx.load_texture(
                    item.id.to_string(),
                    color_image,
                    egui::TextureOptions::LINEAR,
                );
                textures.insert(item.id, texture);
            }
        }

        if let Some(texture) = textures.get(&item.id) {
            let target = image_fit_rect(rect.shrink(3.0), image.width, image.height);
            ui.painter().image(
                texture.id(),
                target,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                Color32::WHITE,
            );
            return response.on_hover_text("Preview image");
        }
    }

    let icon = match item.kind {
        ClipKind::Text => ActionIcon::Copy,
        ClipKind::Url => ActionIcon::Open,
        ClipKind::FilePath => ActionIcon::Finder,
        ClipKind::Email => ActionIcon::Email,
        ClipKind::Phone => ActionIcon::Phone,
        ClipKind::Image => ActionIcon::Preview,
        ClipKind::Files => ActionIcon::Finder,
    };
    let hover_text = match item.kind {
        ClipKind::Text => "Copy text to clipboard",
        ClipKind::Url => "Open URL",
        ClipKind::FilePath => "Reveal in Finder",
        ClipKind::Email => "Compose email",
        ClipKind::Phone => "Open phone handler",
        ClipKind::Image => "Preview image",
        ClipKind::Files => "Reveal in Finder",
    };
    draw_action_icon(
        ui.painter(),
        rect.shrink(13.0),
        icon,
        muted_text(theme),
        1.8,
    );
    response.on_hover_text(hover_text)
}

fn action_tile_size(item: &ClipItem) -> egui::Vec2 {
    if item.kind == ClipKind::Image {
        egui::vec2(ROW_HEIGHT, ROW_HEIGHT)
    } else {
        egui::vec2(THUMBNAIL_SIZE, THUMBNAIL_SIZE)
    }
}

fn image_fit_rect(bounds: egui::Rect, width: usize, height: usize) -> egui::Rect {
    if width == 0 || height == 0 {
        return bounds;
    }

    let image_aspect = width as f32 / height as f32;
    let bounds_aspect = bounds.width() / bounds.height();
    let size = if image_aspect > bounds_aspect {
        egui::vec2(bounds.width(), bounds.width() / image_aspect)
    } else {
        egui::vec2(bounds.height() * image_aspect, bounds.height())
    };

    egui::Rect::from_center_size(bounds.center(), size)
}

fn load_color_image(path: &Path) -> Result<egui::ColorImage> {
    let image = ImageReader::open(path)
        .context("open image")?
        .decode()
        .context("decode image")?
        .to_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    Ok(egui::ColorImage::from_rgba_unmultiplied(
        size,
        image.as_raw(),
    ))
}

fn copy_item_to_clipboard(item: &ClipItem, data_dir: &Path) -> Result<()> {
    match item.kind {
        ClipKind::Text | ClipKind::Url | ClipKind::FilePath | ClipKind::Email | ClipKind::Phone => {
            let mut clipboard = Clipboard::new().context("open clipboard")?;
            let text = item.text.as_deref().context("missing text payload")?;
            clipboard
                .set_text(text.to_owned())
                .context("set clipboard text")?;
        }
        ClipKind::Image => {
            let mut clipboard = Clipboard::new().context("open clipboard")?;
            let image = item.image.as_ref().context("missing image payload")?;
            let rgba = ImageReader::open(data_dir.join(&image.path))
                .context("open stored image")?
                .decode()
                .context("decode stored image")?
                .to_rgba8();
            clipboard
                .set_image(ImageData {
                    width: rgba.width() as usize,
                    height: rgba.height() as usize,
                    bytes: Cow::Owned(rgba.into_raw()),
                })
                .context("set clipboard image")?;
        }
        ClipKind::Files => {
            write_file_urls_to_clipboard(&item.files)?;
        }
    }
    Ok(())
}

fn write_file_urls_to_clipboard(paths: &[PathBuf]) -> Result<()> {
    let objects = file_pasteboard_objects(paths)?;
    let pasteboard = NSPasteboard::generalPasteboard();
    pasteboard.clearContents();
    if pasteboard.writeObjects(&objects) {
        Ok(())
    } else {
        anyhow::bail!("NSPasteboard refused file URLs")
    }
}

fn file_pasteboard_objects(
    paths: &[PathBuf],
) -> Result<Retained<NSArray<ProtocolObject<dyn NSPasteboardWriting>>>> {
    let urls = paths
        .iter()
        .filter_map(|path| {
            let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
            NSURL::from_file_path(canonical).map(ProtocolObject::from_retained)
        })
        .collect::<Vec<Retained<ProtocolObject<dyn NSPasteboardWriting>>>>();

    if urls.is_empty() {
        anyhow::bail!("missing file payload")
    }

    Ok(NSArray::from_retained_slice(&urls))
}

fn open_share_sheet(item: &ClipItem, data_dir: &Path) -> Result<Retained<NSSharingServicePicker>> {
    let share_items = share_items_for_clip(item, data_dir)?;
    if share_items.is_empty() {
        anyhow::bail!("missing share payload");
    }

    let items = NSArray::from_retained_slice(&share_items);
    let picker =
        unsafe { NSSharingServicePicker::initWithItems(NSSharingServicePicker::alloc(), &items) };
    let mtm = MainThreadMarker::new().context("share sheet must run on the main thread")?;
    let app = NSApplication::sharedApplication(mtm);
    let window = app
        .keyWindow()
        .or_else(|| app.mainWindow())
        .context("share sheet anchor window unavailable")?;
    let view = window
        .contentView()
        .context("share sheet anchor view unavailable")?;
    let frame = view.frame();
    let anchor = NSRect::new(
        NSPoint::new(frame.size.width * 0.5, frame.size.height * 0.25),
        NSSize::new(1.0, 1.0),
    );
    picker.showRelativeToRect_ofView_preferredEdge(anchor, &view, NSRectEdge::MaxY);

    Ok(picker)
}

fn share_items_for_clip(item: &ClipItem, data_dir: &Path) -> Result<Vec<Retained<AnyObject>>> {
    match item.kind {
        ClipKind::Text | ClipKind::Email | ClipKind::Phone => {
            let text = item.text.as_deref().context("missing text payload")?;
            Ok(vec![retained_as_any(NSString::from_str(text))])
        }
        ClipKind::Url => {
            let text = item.text.as_deref().context("missing URL payload")?;
            let url = share_url_from_text(text)
                .unwrap_or_else(|| retained_as_any(NSString::from_str(text)));
            Ok(vec![url])
        }
        ClipKind::FilePath => {
            let paths = file_paths_for_item(item);
            share_file_urls(&paths)
        }
        ClipKind::Files => share_file_urls(&item.files),
        ClipKind::Image => {
            let image = item.image.as_ref().context("missing image payload")?;
            share_file_urls(&[data_dir.join(&image.path)])
        }
    }
}

fn share_url_from_text(value: &str) -> Option<Retained<AnyObject>> {
    let string = NSString::from_str(value.trim());
    NSURL::URLWithString(&string).map(retained_as_any)
}

fn share_file_urls(paths: &[PathBuf]) -> Result<Vec<Retained<AnyObject>>> {
    let items = paths
        .iter()
        .filter_map(|path| {
            let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
            NSURL::from_file_path(canonical).map(retained_as_any)
        })
        .collect::<Vec<_>>();

    if items.is_empty() {
        anyhow::bail!("missing file payload")
    }

    Ok(items)
}

fn retained_as_any<T: Message>(object: Retained<T>) -> Retained<AnyObject> {
    unsafe { Retained::cast_unchecked(object) }
}

fn frontmost_application_pid() -> Option<i32> {
    let app = NSWorkspace::sharedWorkspace().frontmostApplication()?;
    let pid = app.processIdentifier();
    let current_pid = NSRunningApplication::currentApplication().processIdentifier();
    if pid <= 0 || pid == current_pid {
        None
    } else {
        Some(pid)
    }
}

fn activate_application(pid: i32) -> bool {
    let Some(app) = NSRunningApplication::runningApplicationWithProcessIdentifier(pid) else {
        return false;
    };
    if app.isTerminated() {
        return false;
    }
    let _ = app.unhide();
    app.activateWithOptions(NSApplicationActivationOptions::ActivateAllWindows)
}

fn paste_current_clipboard(target_pid: Option<i32>) -> Result<()> {
    const KEY_V: u16 = 9;

    if let Some(pid) = target_pid {
        let _ = activate_application(pid);
        thread::sleep(Duration::from_millis(120));
    }

    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|_| anyhow::anyhow!("create keyboard event source"))?;
    let key_down = CGEvent::new_keyboard_event(source.clone(), KEY_V, true)
        .map_err(|_| anyhow::anyhow!("create paste key down event"))?;
    let key_up = CGEvent::new_keyboard_event(source, KEY_V, false)
        .map_err(|_| anyhow::anyhow!("create paste key up event"))?;

    key_down.set_flags(CGEventFlags::CGEventFlagCommand);
    key_up.set_flags(CGEventFlags::CGEventFlagCommand);
    if let Some(pid) = target_pid {
        key_down.post_to_pid(pid);
        key_up.post_to_pid(pid);
    } else {
        key_down.post(CGEventTapLocation::HID);
        key_up.post(CGEventTapLocation::HID);
    }
    Ok(())
}

fn accessibility_permission_granted() -> bool {
    unsafe { AXIsProcessTrusted() }
}

fn request_accessibility_permission() -> bool {
    let key = CFString::from_static_string("AXTrustedCheckOptionPrompt");
    let value = CFBoolean::true_value();
    let options = CFDictionary::from_CFType_pairs(&[(key, value)]);
    unsafe { AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef()) }
}

fn launch_agent_enabled() -> bool {
    launch_agent_path().is_some_and(|path| path.exists())
}

fn set_launch_agent_enabled(enabled: bool) -> Result<()> {
    let path = launch_agent_path().context("resolve LaunchAgents path")?;
    if enabled {
        let parent = path.parent().context("resolve LaunchAgents directory")?;
        fs::create_dir_all(parent).context("create LaunchAgents directory")?;
        let executable = env::current_exe().context("resolve current executable")?;
        let plist = launch_agent_plist(&executable);
        fs::write(&path, plist).context("write LaunchAgent plist")?;
    } else if path.exists() {
        fs::remove_file(&path).context("remove LaunchAgent plist")?;
    }
    Ok(())
}

fn launch_agent_path() -> Option<PathBuf> {
    let home = env::var_os("HOME")?;
    Some(
        PathBuf::from(home)
            .join("Library")
            .join("LaunchAgents")
            .join(format!("{LAUNCH_AGENT_LABEL}.plist")),
    )
}

fn launch_agent_plist(executable: &Path) -> String {
    let executable = xml_escape(&executable.to_string_lossy());
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{LAUNCH_AGENT_LABEL}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{executable}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
</dict>
</plist>
"#
    )
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn normalize_loaded_item(item: &mut ClipItem) -> bool {
    if let Some(text) = item.text.as_deref() {
        let kind = classify_text(text);
        let summary = summary_for_text(text);
        let changed = item.kind != kind || item.summary != summary;
        item.kind = kind;
        item.summary = summary;
        return changed;
    }

    if item.image.is_some() && item.kind != ClipKind::Image {
        item.kind = ClipKind::Image;
        return true;
    }

    if !item.files.is_empty() {
        let summary = summary_for_files(&item.files);
        let changed = item.kind != ClipKind::Files || item.summary != summary;
        item.kind = ClipKind::Files;
        item.summary = summary;
        return changed;
    }

    false
}

fn summary_for_text(text: &str) -> String {
    summarize_text(&mask_sensitive_text(text))
}

fn summary_for_files(paths: &[PathBuf]) -> String {
    match paths {
        [] => "No files".to_owned(),
        [path] => path
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_owned)
            .unwrap_or_else(|| path.display().to_string()),
        [first, rest @ ..] => {
            let name = first
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::to_owned)
                .unwrap_or_else(|| first.display().to_string());
            format!("{name} + {} more", rest.len())
        }
    }
}

fn classify_text(text: &str) -> ClipKind {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return ClipKind::Text;
    }

    if file_path_from_text(trimmed).is_some() {
        ClipKind::FilePath
    } else if Url::parse(trimmed).is_ok() {
        ClipKind::Url
    } else if is_email_address(trimmed) {
        ClipKind::Email
    } else if is_phone_number(trimmed) {
        ClipKind::Phone
    } else {
        ClipKind::Text
    }
}

fn file_path_from_text(text: &str) -> Option<PathBuf> {
    let trimmed = strip_wrapping_quotes(text.trim());
    if trimmed.is_empty() || trimmed.contains('\n') || trimmed.contains('\r') {
        return None;
    }

    if let Ok(url) = Url::parse(trimmed) {
        if url.scheme() == "file" {
            let path = url.to_file_path().ok()?;
            return path.exists().then_some(path);
        }
    }

    let path = if trimmed == "~" {
        PathBuf::from(env::var_os("HOME")?)
    } else if let Some(rest) = trimmed.strip_prefix("~/") {
        PathBuf::from(env::var_os("HOME")?).join(rest)
    } else {
        PathBuf::from(trimmed)
    };

    let candidate = if path.is_absolute() {
        path
    } else if trimmed.starts_with("./") || trimmed.starts_with("../") {
        env::current_dir().ok()?.join(path)
    } else {
        return None;
    };

    candidate.exists().then_some(candidate)
}

fn file_paths_for_item(item: &ClipItem) -> Vec<PathBuf> {
    if item.kind == ClipKind::Files {
        return item.files.clone();
    }

    item.text
        .as_deref()
        .and_then(file_path_from_text)
        .into_iter()
        .collect()
}

fn strip_wrapping_quotes(value: &str) -> &str {
    if value.len() < 2 {
        return value;
    }

    let quoted = (value.starts_with('"') && value.ends_with('"'))
        || (value.starts_with('\'') && value.ends_with('\''));
    if quoted {
        &value[1..value.len() - 1]
    } else {
        value
    }
}

fn is_email_address(value: &str) -> bool {
    if value.contains(char::is_whitespace) {
        return false;
    }

    let Some((local, domain)) = value.split_once('@') else {
        return false;
    };
    if local.is_empty()
        || domain.is_empty()
        || domain.starts_with('.')
        || domain.ends_with('.')
        || !domain.contains('.')
    {
        return false;
    }

    let local_ok = local
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '%' | '+' | '-'));
    let domain_ok = domain
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-'));
    local_ok && domain_ok
}

fn is_phone_number(value: &str) -> bool {
    if contains_luhn_card_number(value) || value.contains('\n') || value.contains('\r') {
        return false;
    }

    let mut digits = 0;
    let mut seen_plus = false;
    for (index, ch) in value.trim().chars().enumerate() {
        if ch.is_ascii_digit() {
            digits += 1;
        } else if ch == '+' {
            if index != 0 || seen_plus {
                return false;
            }
            seen_plus = true;
        } else if !matches!(ch, ' ' | '-' | '.' | '(' | ')') {
            return false;
        }
    }

    (7..=15).contains(&digits)
}

fn phone_url(value: &str) -> Option<String> {
    if !is_phone_number(value) {
        return None;
    }

    let mut phone = String::new();
    for ch in value.trim().chars() {
        if ch.is_ascii_digit() || (ch == '+' && phone.is_empty()) {
            phone.push(ch);
        }
    }
    Some(format!("tel:{phone}"))
}

fn mask_sensitive_text(text: &str) -> String {
    mask_api_tokens(&mask_credit_card_numbers(text))
}

fn mask_credit_card_numbers(text: &str) -> String {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut result = String::new();
    let mut last = 0;
    let mut index = 0;

    while index < chars.len() {
        let (start, ch) = chars[index];
        if !ch.is_ascii_digit() {
            index += 1;
            continue;
        }

        let mut scan = index;
        let mut end = start + ch.len_utf8();
        let mut digits = String::new();
        while scan < chars.len() {
            let (byte_index, scan_ch) = chars[scan];
            if scan_ch.is_ascii_digit() {
                digits.push(scan_ch);
                end = byte_index + scan_ch.len_utf8();
                scan += 1;
            } else if matches!(scan_ch, ' ' | '-')
                && !digits.is_empty()
                && chars
                    .get(scan + 1)
                    .is_some_and(|(_, next)| next.is_ascii_digit())
            {
                end = byte_index + scan_ch.len_utf8();
                scan += 1;
            } else {
                break;
            }

            if digits.len() > 19 {
                break;
            }
        }

        if (13..=19).contains(&digits.len()) && luhn_valid(&digits) {
            result.push_str(&text[last..start]);
            result.push_str(&mask_card_number(&digits));
            last = end;
            index = scan;
        } else {
            index += 1;
        }
    }

    result.push_str(&text[last..]);
    result
}

fn contains_luhn_card_number(text: &str) -> bool {
    mask_credit_card_numbers(text) != text
}

fn luhn_valid(digits: &str) -> bool {
    let mut chars = digits.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if chars.all(|ch| ch == first) {
        return false;
    }

    let mut sum = 0;
    let mut double = false;
    for ch in digits.chars().rev() {
        let Some(mut digit) = ch.to_digit(10) else {
            return false;
        };
        if double {
            digit *= 2;
            if digit > 9 {
                digit -= 9;
            }
        }
        sum += digit;
        double = !double;
    }

    sum % 10 == 0
}

fn mask_card_number(digits: &str) -> String {
    let last_four = last_chars(digits, 4);
    format!("•••• •••• •••• {last_four}")
}

fn mask_api_tokens(text: &str) -> String {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut result = String::new();
    let mut last = 0;
    let mut index = 0;

    while index < chars.len() {
        let (start, ch) = chars[index];
        if !is_secret_token_char(ch) {
            index += 1;
            continue;
        }

        let mut scan = index;
        let mut end = start + ch.len_utf8();
        while scan < chars.len() {
            let (byte_index, scan_ch) = chars[scan];
            if is_secret_token_char(scan_ch) {
                end = byte_index + scan_ch.len_utf8();
                scan += 1;
            } else {
                break;
            }
        }

        let token = &text[start..end];
        if let Some(masked) = mask_api_token(token, sensitive_context_before(text, start)) {
            result.push_str(&text[last..start]);
            result.push_str(&masked);
            last = end;
        }
        index = scan;
    }

    result.push_str(&text[last..]);
    result
}

fn is_secret_token_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_')
}

fn mask_api_token(token: &str, sensitive_context: bool) -> Option<String> {
    const PREFIXES: &[&str] = &[
        "sk-proj-",
        "sk-",
        "ghp_",
        "github_pat_",
        "AKIA",
        "xoxb-",
        "xoxp-",
        "AIza",
    ];

    if let Some(prefix) = PREFIXES
        .iter()
        .copied()
        .find(|prefix| token.starts_with(prefix) && token.len() >= prefix.len() + 8)
    {
        return Some(format!("{prefix}...{}", last_chars(token, 4)));
    }

    if sensitive_context && looks_like_secret_value(token) {
        return Some(format!("••••{}", last_chars(token, 4)));
    }

    None
}

fn sensitive_context_before(text: &str, start: usize) -> bool {
    let context: String = text[..start]
        .chars()
        .rev()
        .take(48)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>()
        .to_ascii_lowercase();

    [
        "api_key",
        "apikey",
        "access_key",
        "secret",
        "token",
        "password",
        "passwd",
        "credential",
    ]
    .iter()
    .any(|label| context.contains(label))
}

fn looks_like_secret_value(token: &str) -> bool {
    if token.len() < 32 {
        return false;
    }

    let has_alpha = token.chars().any(|ch| ch.is_ascii_alphabetic());
    let has_digit = token.chars().any(|ch| ch.is_ascii_digit());
    has_alpha && has_digit
}

fn last_chars(value: &str, count: usize) -> String {
    let mut chars = value.chars().rev().take(count).collect::<Vec<_>>();
    chars.reverse();
    chars.into_iter().collect()
}

fn summarize_text(text: &str) -> String {
    const LIMIT: usize = 180;
    let compact = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.chars().count() <= LIMIT {
        compact
    } else {
        let mut summary = compact.chars().take(LIMIT).collect::<String>();
        summary.push_str("...");
        summary
    }
}

fn default_data_dir() -> PathBuf {
    ProjectDirs::from("com", "mgosal", APP_NAME)
        .map(|dirs| dirs.data_local_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from(".better-clipboard"))
}

fn history_path(data_dir: &Path) -> PathBuf {
    data_dir.join("history.json")
}

fn settings_path(data_dir: &Path) -> PathBuf {
    data_dir.join("settings.json")
}

fn images_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("images")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_history_keeps_multiple_distinct_items() {
        let data_dir =
            std::env::temp_dir().join(format!("better-clipboard-test-{}", Uuid::new_v4()));
        let mut store = HistoryStore::new(data_dir.clone(), DEFAULT_HISTORY_LIMIT);

        store
            .push(text_snapshot("first copied item"))
            .expect("push first item");
        store
            .push(text_snapshot("second copied item"))
            .expect("push second item");

        assert_eq!(store.items.len(), 2);
        assert_eq!(store.items[0].text.as_deref(), Some("second copied item"));
        assert_eq!(store.items[1].text.as_deref(), Some("first copied item"));

        let _ = fs::remove_dir_all(data_dir);
    }

    #[test]
    fn suppressed_copy_does_not_promote_existing_item() {
        let data_dir = std::env::temp_dir().join(format!(
            "better-clipboard-suppressed-copy-test-{}",
            Uuid::new_v4()
        ));
        let mut store = HistoryStore::new(data_dir.clone(), DEFAULT_HISTORY_LIMIT);

        store.push(text_snapshot("first")).expect("push first item");
        store
            .push(text_snapshot("second"))
            .expect("push second item");

        let first_again = text_snapshot("first");
        store.suppress_next_hash(first_again.hash());
        store.push(first_again).expect("push suppressed item");

        assert_eq!(store.items.len(), 2);
        assert_eq!(store.items[0].text.as_deref(), Some("second"));
        assert_eq!(store.items[1].text.as_deref(), Some("first"));
        assert!(store.suppressed_hashes.is_empty());

        let _ = fs::remove_dir_all(data_dir);
    }

    #[test]
    fn classifies_common_text_payloads() {
        assert_eq!(classify_text("https://example.com"), ClipKind::Url);
        assert_eq!(classify_text("person@example.com"), ClipKind::Email);
        assert_eq!(classify_text("+44 20 7946 0958"), ClipKind::Phone);
    }

    #[test]
    fn classifies_existing_file_paths() {
        let path = std::env::temp_dir().join(format!("better-clipboard-file-{}", Uuid::new_v4()));
        fs::write(&path, "test").expect("write temp file");

        assert_eq!(
            classify_text(path.to_str().expect("utf-8 temp path")),
            ClipKind::FilePath
        );

        let _ = fs::remove_file(path);
    }

    #[test]
    fn parses_existing_file_urls() {
        let path =
            std::env::temp_dir().join(format!("better-clipboard-file-url-{}", Uuid::new_v4()));
        fs::write(&path, "test").expect("write temp file");
        let url = Url::from_file_path(&path).expect("file URL");

        assert_eq!(
            file_path_from_file_url_string(url.as_str()),
            Some(path.clone())
        );

        let _ = fs::remove_file(path);
    }

    #[test]
    fn file_history_items_keep_paths_without_copying_files() {
        let data_dir = std::env::temp_dir().join(format!(
            "better-clipboard-file-history-test-{}",
            Uuid::new_v4()
        ));
        let file_a = data_dir.join("first.txt");
        let file_b = data_dir.join("second.txt");
        fs::create_dir_all(&data_dir).expect("create temp dir");
        fs::write(&file_a, "first").expect("write first file");
        fs::write(&file_b, "second").expect("write second file");
        let mut store = HistoryStore::new(data_dir.clone(), DEFAULT_HISTORY_LIMIT);

        store
            .push(file_snapshot(&[file_a.clone(), file_b.clone()]))
            .expect("push file item");

        assert_eq!(store.items.len(), 1);
        assert_eq!(store.items[0].kind, ClipKind::Files);
        assert_eq!(store.items[0].files, vec![file_a, file_b]);
        assert_eq!(store.items[0].summary, "first.txt + 1 more");

        let _ = fs::remove_dir_all(data_dir);
    }

    #[test]
    fn masks_sensitive_display_values_without_changing_raw_text() {
        let raw = "card 4111 1111 1111 1111 and key sk-proj-abcdefghijklmnopqrstuvwxyz123456";
        let item = ClipItem::from_text(raw.to_owned(), "hash".to_owned());

        assert_eq!(item.text.as_deref(), Some(raw));
        assert!(item.summary.contains("•••• •••• •••• 1111"));
        assert!(item.summary.contains("sk-proj-...3456"));
        assert!(!item.summary.contains("4111 1111 1111 1111"));
        assert!(!item.summary.contains("abcdefghijklmnopqrstuvwxyz"));
    }

    fn text_snapshot(text: &str) -> ClipboardSnapshot {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"text:");
        hasher.update(text.as_bytes());
        ClipboardSnapshot::Text {
            text: text.to_owned(),
            hash: hasher.finalize().to_hex().to_string(),
        }
    }

    fn file_snapshot(paths: &[PathBuf]) -> ClipboardSnapshot {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"files:");
        for path in paths {
            hasher.update(path.to_string_lossy().as_bytes());
            hasher.update(b"\0");
        }
        ClipboardSnapshot::Files {
            paths: paths.to_vec(),
            hash: hasher.finalize().to_hex().to_string(),
        }
    }
}
