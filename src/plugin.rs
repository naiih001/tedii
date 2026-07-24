use std::collections::HashMap;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use mlua::{Lua, RegistryKey};

use crate::editor::Editor;

const PLUGIN_DIR: &str = "tedii/plugins";

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(dead_code)]
pub enum EventKind {
    OpenFile,
    SaveFile,
    CursorMoved,
    ModeChanged,
    BufferChanged,
    DiagnosticsUpdated,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[allow(dead_code)]
pub enum OverlayLayer {
    BelowPopups,
    AbovePopups,
    Topmost,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct PluginInfo {
    pub name: String,
    pub path: PathBuf,
    pub error: Option<String>,
}

pub struct PluginRuntime {
    lua: Lua,
    plugins: Vec<PluginInfo>,
    events: HashMap<EventKind, Vec<RegistryKey>>,
    commands: HashMap<String, RegistryKey>,
    keybindings: HashMap<String, Vec<(KeyEvent, RegistryKey)>>,
    overlays: Vec<OverlayRegistration>,
    status_items: Vec<StatusItemRegistration>,
    pending: VecDeque<PendingCoroutine>,
    editor_ptr: Option<usize>,
}

struct PendingCoroutine {
    thread: mlua::Thread,
    wake_at: Option<Instant>,
}

#[allow(dead_code)]
pub struct OverlayRegistration {
    pub name: String,
    pub layer: OverlayLayer,
    pub callback: RegistryKey,
}

#[allow(dead_code)]
struct StatusItemRegistration {
    name: String,
    priority: i32,
    callback: RegistryKey,
}

fn parse_key_event(s: &str) -> Option<KeyEvent> {
    if s.is_empty() {
        return None;
    }

    if !s.starts_with('<') && s.len() == 1 {
        if let Some(c) = s.chars().next() {
            return Some(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
        }
    }

    if s.starts_with('<') && s.ends_with('>') {
        let inner = &s[1..s.len() - 1];
        let (mods, key_name) = parse_modifiers(inner);
        let code = match key_name.to_lowercase().as_str() {
            "esc" | "escape" => KeyCode::Esc,
            "enter" | "cr" => KeyCode::Enter,
            "tab" => KeyCode::Tab,
            "backspace" | "bs" => KeyCode::Backspace,
            "up" => KeyCode::Up,
            "down" => KeyCode::Down,
            "left" => KeyCode::Left,
            "right" => KeyCode::Right,
            "home" => KeyCode::Home,
            "end" => KeyCode::End,
            "pageup" | "pgup" => KeyCode::PageUp,
            "pagedown" | "pgdn" => KeyCode::PageDown,
            "del" | "delete" => KeyCode::Delete,
            "space" => KeyCode::Char(' '),
            "backtab" => KeyCode::BackTab,
            _ if key_name.len() == 1 => {
                if let Some(c) = key_name.chars().next() {
                    KeyCode::Char(c)
                } else {
                    return None;
                }
            }
            _ => return None,
        };
        return Some(KeyEvent::new(code, mods));
    }

    None
}

fn parse_modifiers(s: &str) -> (KeyModifiers, &str) {
    let mut mods = KeyModifiers::NONE;
    let mut rest = s;

    loop {
        if rest.starts_with("C-") || rest.starts_with("c-") {
            mods.insert(KeyModifiers::CONTROL);
            rest = &rest[2..];
        } else if rest.starts_with("A-") || rest.starts_with("a-") {
            mods.insert(KeyModifiers::ALT);
            rest = &rest[2..];
        } else if rest.starts_with("S-") || rest.starts_with("s-") {
            mods.insert(KeyModifiers::SHIFT);
            rest = &rest[2..];
        } else {
            break;
        }
    }

    (mods, rest)
}

fn mode_key(mode: &str) -> Option<&'static str> {
    match mode {
        "normal" => Some("normal"),
        "insert" => Some("insert"),
        "visual" => Some("visual"),
        "command" => Some("command"),
        "search" => Some("search"),
        _ => None,
    }
}

fn parse_key_sequence(s: &str) -> Option<KeyEvent> {
    // Try to parse the whole string as a single key event.
    // This handles both simple chars like "j" and angle-bracket syntax like "<C-p>".
    parse_key_event(s)
}

fn default_plugins_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join(PLUGIN_DIR))
}

impl PluginRuntime {
    pub fn new() -> anyhow::Result<Self> {
        let lua = Lua::new();

        let tedii = lua.create_table()?;
        tedii.set("version", "0.1.0")?;

        let log_fn = lua.create_function(|_, msg: String| {
            crate::lsp::log_line(format!("[plugin] {}", msg));
            Ok(())
        })?;
        tedii.set("log", log_fn)?;

        let plugin_dir_fn = lua.create_function(|lua, ()| {
            let globals = lua.globals();
            let val: mlua::Value = globals.get("_TEDII_PLUGIN_DIR")?;
            match val {
                mlua::Value::String(s) => Ok(Some(s.to_str()?.to_string())),
                _ => Ok(None),
            }
        })?;
        tedii.set("plugin_dir", plugin_dir_fn)?;

        lua.globals().set("tedii", tedii)?;

        Ok(PluginRuntime {
            lua,
            plugins: Vec::new(),
            events: HashMap::new(),
            commands: HashMap::new(),
            keybindings: HashMap::new(),
            overlays: Vec::new(),
            status_items: Vec::new(),
            pending: VecDeque::new(),
            editor_ptr: None,
        })
    }

    pub fn init_api(&mut self) -> anyhow::Result<()> {
        let rt_ptr = self as *mut PluginRuntime as usize;
        let tedii: mlua::Table = self.lua.globals().get("tedii")?;

        let defer_fn = self.lua.create_function(move |_, cb: mlua::Function| {
            let rt = unsafe { &mut *(rt_ptr as *mut PluginRuntime) };
            let thread = rt.lua.create_thread(cb)?;
            rt.pending.push_back(PendingCoroutine {
                thread,
                wake_at: None,
            });
            Ok(())
        })?;
        tedii.set("defer", defer_fn)?;

        let rt_ptr2 = self as *mut PluginRuntime as usize;
        let schedule_fn = self.lua.create_function(move |_, (delay, cb): (f64, mlua::Function)| {
            let rt = unsafe { &mut *(rt_ptr2 as *mut PluginRuntime) };
            let thread = rt.lua.create_thread(cb)?;
            let wake_at = Instant::now()
                .checked_add(std::time::Duration::from_secs_f64(delay))
                .unwrap_or_else(|| Instant::now() + std::time::Duration::from_secs(3600));
            rt.pending.push_back(PendingCoroutine {
                thread,
                wake_at: Some(wake_at),
            });
            Ok(())
        })?;
        tedii.set("schedule", schedule_fn)?;

        let events = self.register_events_api(rt_ptr)?;
        tedii.set("events", events)?;

        let commands = self.register_commands_api(rt_ptr)?;
        tedii.set("commands", commands)?;

        let keymap = self.register_keymap_api(rt_ptr)?;
        tedii.set("keymap", keymap)?;

        let ui = self.register_ui_api(rt_ptr)?;
        tedii.set("ui", ui)?;

        Ok(())
    }

    fn run_threaded(&mut self, func: mlua::Function) -> bool {
        match self.lua.create_thread(func) {
            Ok(thread) => {
                let result = thread.resume::<mlua::Value>(());
                match result {
                    Ok(_) => {
                        if thread.status() == mlua::ThreadStatus::Resumable {
                            self.pending.push_back(PendingCoroutine {
                                thread,
                                wake_at: None,
                            });
                        }
                        true
                    }
                    Err(_) => false,
                }
            }
            Err(_) => false,
        }
    }

    pub fn register_editor(&mut self, editor: &Editor) -> anyhow::Result<()> {
        let ptr = editor as *const Editor as usize;
        self.editor_ptr = Some(ptr);

        let tedii: mlua::Table = self.lua.globals().get("tedii")?;

        let buf = self.register_buf_api(ptr)?;
        tedii.set("buf", buf)?;

        let cursor = self.register_cursor_api(ptr)?;
        tedii.set("cursor", cursor)?;

        Ok(())
    }

    fn register_events_api(&self, rt_ptr: usize) -> anyhow::Result<mlua::Table> {
        let events = self.lua.create_table()?;

        let kinds = [
            ("on_open", EventKind::OpenFile),
            ("on_save", EventKind::SaveFile),
            ("on_cursor_moved", EventKind::CursorMoved),
            ("on_mode_changed", EventKind::ModeChanged),
            ("on_buffer_changed", EventKind::BufferChanged),
            ("on_diagnostics_updated", EventKind::DiagnosticsUpdated),
        ];
        for (name, kind) in &kinds {
            let kind = kind.clone();
            let func = self.lua.create_function(move |_, cb: mlua::Function| {
                let rt = unsafe { &mut *(rt_ptr as *mut PluginRuntime) };
                let key = rt.lua.create_registry_value(cb)?;
                rt.events.entry(kind.clone()).or_default().push(key);
                Ok(())
            })?;
            events.set(*name, func)?;
        }

        Ok(events)
    }

    fn register_commands_api(&self, rt_ptr: usize) -> anyhow::Result<mlua::Table> {
        let commands = self.lua.create_table()?;

        let register_fn = self.lua.create_function(move |_, (name, cb): (String, mlua::Function)| {
            let rt = unsafe { &mut *(rt_ptr as *mut PluginRuntime) };
            let key = rt.lua.create_registry_value(cb)?;
            rt.commands.insert(name, key);
            Ok(())
        })?;
        commands.set("register", register_fn)?;

        let run_fn = self.lua.create_function(move |_, name: String| {
            let rt = unsafe { &*(rt_ptr as *mut PluginRuntime) };
            let Some(key) = rt.commands.get(&name) else {
                return Err(mlua::Error::external(format!("unknown command: {}", name)));
            };
            let func = rt.lua.registry_value::<mlua::Function>(key)?;
            func.call::<()>(())
        })?;
        commands.set("run", run_fn)?;

        Ok(commands)
    }

    fn register_keymap_api(&self, rt_ptr: usize) -> anyhow::Result<mlua::Table> {
        let keymap = self.lua.create_table()?;

        let set_fn = self.lua.create_function(move |_, (mode, key_str, cb): (String, String, mlua::Function)| {
            let mode_key = mode_key(&mode).ok_or_else(|| {
                mlua::Error::external(format!("unknown mode: {}", mode))
            })?;
            let key = parse_key_sequence(&key_str).ok_or_else(|| {
                mlua::Error::external(format!("invalid key: {}", key_str))
            })?;
            let rt = unsafe { &mut *(rt_ptr as *mut PluginRuntime) };
            let registry_key = rt.lua.create_registry_value(cb)?;
            rt.keybindings
                .entry(mode_key.to_string())
                .or_default()
                .push((key, registry_key));
            Ok(())
        })?;
        keymap.set("set", set_fn)?;

        let get_fn = self.lua.create_function(move |_, (mode, key_str): (String, String)| {
            let rt = unsafe { &*(rt_ptr as *mut PluginRuntime) };
            let has = mode_key(&mode)
                .and_then(|mk| rt.keybindings.get(mk))
                .and_then(|b| parse_key_sequence(&key_str).map(|k| b.iter().any(|(bk, _)| bk == &k)))
                .unwrap_or(false);
            Ok(has)
        })?;
        keymap.set("get", get_fn)?;

        let remove_fn = self.lua.create_function(move |_, (mode, key_str): (String, String)| {
            let rt = unsafe { &mut *(rt_ptr as *mut PluginRuntime) };
            let mode_key = mode_key(&mode).ok_or_else(|| {
                mlua::Error::external(format!("unknown mode: {}", mode))
            })?;
            let key = parse_key_sequence(&key_str).ok_or_else(|| {
                mlua::Error::external(format!("invalid key: {}", key_str))
            })?;
            if let Some(bindings) = rt.keybindings.get_mut(mode_key) {
                bindings.retain(|(k, _)| k != &key);
            }
            Ok(())
        })?;
        keymap.set("remove", remove_fn)?;

        Ok(keymap)
    }

    fn register_ui_api(&self, rt_ptr: usize) -> anyhow::Result<mlua::Table> {
        let ui = self.lua.create_table()?;

        let add_status_fn = self.lua.create_function(
            move |_, (name, cb, options): (String, mlua::Function, mlua::Table)| {
                let priority: i32 = options.get("priority").unwrap_or(100);
                let rt = unsafe { &mut *(rt_ptr as *mut PluginRuntime) };
                let key = rt.lua.create_registry_value(cb)?;
                rt.status_items.push(StatusItemRegistration {
                    name,
                    priority,
                    callback: key,
                });
                rt.status_items.sort_by_key(|s| std::cmp::Reverse(s.priority));
                Ok(())
            },
        )?;
        ui.set("add_status_item", add_status_fn)?;

        let add_overlay_fn = self.lua.create_function(
            move |_, (name, cb, options): (String, mlua::Function, mlua::Table)| {
                let layer_str: String = options.get("layer").unwrap_or("above_popups".to_string());
                let layer = match layer_str.as_str() {
                    "below_popups" => OverlayLayer::BelowPopups,
                    "topmost" => OverlayLayer::Topmost,
                    _ => OverlayLayer::AbovePopups,
                };
                let rt = unsafe { &mut *(rt_ptr as *mut PluginRuntime) };
                let key = rt.lua.create_registry_value(cb)?;
                rt.overlays.push(OverlayRegistration {
                    name,
                    layer,
                    callback: key,
                });
                Ok(())
            },
        )?;
        ui.set("add_overlay", add_overlay_fn)?;

        Ok(ui)
    }

    pub fn render_status_items(&self) -> Vec<String> {
        let mut items = Vec::new();
        for item in &self.status_items {
            if let Ok(func) = self.lua.registry_value::<mlua::Function>(&item.callback) {
                if let Ok(text) = func.call::<String>(()) {
                    if !text.is_empty() {
                        items.push(text);
                    }
                }
            }
        }
        items
    }

    #[allow(dead_code)]
    pub fn render_overlays(&self, layer: OverlayLayer) -> Vec<&OverlayRegistration> {
        self.overlays
            .iter()
            .filter(|o| o.layer == layer)
            .collect()
    }

    pub fn resolve_keybinding(&mut self, mode: &str, event: &KeyEvent) -> bool {
        let Some(bindings) = self.keybindings.get(mode) else {
            return false;
        };
        for (key, reg_key) in bindings {
            if key == event {
                if let Ok(func) = self.lua.registry_value::<mlua::Function>(reg_key) {
                    return self.run_threaded(func);
                }
            }
        }
        false
    }

    #[allow(dead_code)]
    fn editor_from_ptr<T>(&self, f: impl FnOnce(&Editor) -> T) -> T {
        let ptr = self.editor_ptr.expect("editor not registered");
        // SAFETY: single-threaded, editor lives for the program lifetime
        let editor = unsafe { &*(ptr as *const Editor) };
        f(editor)
    }

    #[allow(dead_code)]
    fn editor_from_ptr_mut<T>(&self, f: impl FnOnce(&mut Editor) -> T) -> T {
        let ptr = self.editor_ptr.expect("editor not registered");
        // SAFETY: single-threaded, no aliasing at call site
        let editor = unsafe { &mut *(ptr as *mut Editor) };
        f(editor)
    }

    fn register_buf_api(&self, editor_ptr: usize) -> anyhow::Result<mlua::Table> {
        let buf = self.lua.create_table()?;

        let get_text = self.lua.create_function(move |_, (start_line, end_line): (i64, i64)| {
            let ptr = editor_ptr as *const Editor;
            // SAFETY: single-threaded, editor lives for program lifetime
            let editor = unsafe { &*ptr };
            let end_line = if end_line < 0 {
                editor.buffer.len_lines() as i64
            } else {
                end_line
            };
            let start_char = editor.buffer.line_to_char(start_line as usize);
            let end_char = if end_line as usize >= editor.buffer.len_lines() {
                editor.buffer.len_chars()
            } else {
                editor.buffer.line_to_char(end_line as usize)
            };
            let text = editor.buffer.slice(start_char..end_char).to_string();
            Ok(text)
        })?;
        buf.set("get_text", get_text)?;

        let set_text = self.lua.create_function(move |_, (start_line, end_line, text): (i64, i64, String)| {
            let ptr = editor_ptr as *mut Editor;
            let editor = unsafe { &mut *ptr };
            let end_line = if end_line < 0 {
                editor.buffer.len_lines() as i64
            } else {
                end_line
            };
            let start_char = editor.buffer.line_to_char(start_line as usize);
            let end_char = if end_line as usize >= editor.buffer.len_lines() {
                editor.buffer.len_chars()
            } else {
                editor.buffer.line_to_char(end_line as usize)
            };
            editor.begin_undo_group();
            editor.buffer.remove(start_char..end_char);
            editor.buffer.insert(start_char, &text);
            editor.cursor = editor.cursor.min(editor.buffer.len_chars());
            editor.buffer_version = editor.buffer_version.wrapping_add(1);
            Ok(())
        })?;
        buf.set("set_text", set_text)?;

        let get_line = self.lua.create_function(move |_, line: i64| {
            let ptr = editor_ptr as *const Editor;
            let editor = unsafe { &*ptr };
            let line_idx = line.max(0) as usize;
            if line_idx >= editor.buffer.len_lines() {
                return Ok(String::new());
            }
            let line_str = editor.buffer.line(line_idx).to_string();
            Ok(line_str)
        })?;
        buf.set("get_line", get_line)?;

        let line_count = self.lua.create_function(move |_, ()| {
            let ptr = editor_ptr as *const Editor;
            let editor = unsafe { &*ptr };
            Ok(editor.buffer.len_lines())
        })?;
        buf.set("line_count", line_count)?;

        let get_name = self.lua.create_function(move |_, ()| {
            let ptr = editor_ptr as *const Editor;
            let editor = unsafe { &*ptr };
            Ok(editor.current_file.as_ref()
                .and_then(|p| p.to_str())
                .unwrap_or("Untitled")
                .to_string())
        })?;
        buf.set("get_name", get_name)?;

        let get_language = self.lua.create_function(move |_, ()| {
            let ptr = editor_ptr as *const Editor;
            let editor = unsafe { &*ptr };
            let lang = editor.current_file.as_ref()
                .and_then(|p| p.to_str())
                .and_then(|p| editor.highlighter.language_for_file(p))
                .unwrap_or_else(|| "unknown".to_string());
            Ok(lang)
        })?;
        buf.set("get_language", get_language)?;

        let get_version = self.lua.create_function(move |_, ()| {
            let ptr = editor_ptr as *const Editor;
            let editor = unsafe { &*ptr };
            Ok(editor.buffer_version)
        })?;
        buf.set("get_version", get_version)?;

        Ok(buf)
    }

    fn register_cursor_api(&self, editor_ptr: usize) -> anyhow::Result<mlua::Table> {
        let cursor = self.lua.create_table()?;

        let get_pos = self.lua.create_function(move |_, ()| {
            let ptr = editor_ptr as *const Editor;
            let editor = unsafe { &*ptr };
            let line = editor.buffer.char_to_line(editor.cursor);
            let col = editor.cursor - editor.buffer.line_to_char(line);
            Ok((line, col))
        })?;
        cursor.set("get_pos", get_pos)?;

        let set_pos = self.lua.create_function(move |_, (line, col): (usize, usize)| {
            let ptr = editor_ptr as *mut Editor;
            let editor = unsafe { &mut *ptr };
            if line >= editor.buffer.len_lines() {
                return Ok(());
            }
            let line_start = editor.buffer.line_to_char(line);
            let line_len = editor.buffer.line(line).len_chars();
            let effective_col = col.min(line_len.saturating_sub(1));
            editor.cursor = line_start + effective_col;
            Ok(())
        })?;
        cursor.set("set_pos", set_pos)?;

        let get_scroll = self.lua.create_function(move |_, ()| {
            let ptr = editor_ptr as *const Editor;
            let editor = unsafe { &*ptr };
            Ok((editor.scroll_x, editor.scroll_y))
        })?;
        cursor.set("get_scroll", get_scroll)?;

        Ok(cursor)
    }

    pub fn execute_command(&mut self, name: &str) -> bool {
        let Some(key) = self.commands.get(name) else {
            return false;
        };
        match self.lua.registry_value::<mlua::Function>(key) {
            Ok(func) => self.run_threaded(func),
            Err(_) => false,
        }
    }

    pub fn fire_event(&self, kind: EventKind) {
        let Some(keys) = self.events.get(&kind) else {
            return;
        };
        for key in keys {
            if let Ok(func) = self.lua.registry_value::<mlua::Function>(key) {
                let _ = func.call::<mlua::Value>(());
            }
        }
    }

    pub fn load_plugins(&mut self) -> Vec<PluginInfo> {
        match default_plugins_dir() {
            Some(dir) => self.load_plugins_from(dir),
            None => {
                crate::lsp::log_line("[plugin] no config dir found; skipping plugin loading");
                Vec::new()
            }
        }
    }

    pub fn load_plugins_from(&mut self, plugins_dir: PathBuf) -> Vec<PluginInfo> {
        let mut results = Vec::new();

        if !plugins_dir.exists() {
            crate::lsp::log_line(format!(
                "[plugin] plugins directory does not exist: {}",
                plugins_dir.display()
            ));
            if let Err(e) = std::fs::create_dir_all(&plugins_dir) {
                crate::lsp::log_line(format!(
                    "[plugin] failed to create plugins directory: {}",
                    e
                ));
            }
            return results;
        }

        let entries = match std::fs::read_dir(&plugins_dir) {
            Ok(e) => e,
            Err(e) => {
                crate::lsp::log_line(format!(
                    "[plugin] failed to read plugins directory: {}",
                    e
                ));
                return results;
            }
        };

        let mut plugin_dirs: Vec<PathBuf> = entries
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
            .map(|e| e.path())
            .collect();
        plugin_dirs.sort();

        for dir in &plugin_dirs {
            let name = dir
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string();
            let init_path = dir.join("init.lua");

            if !init_path.exists() {
                crate::lsp::log_line(format!(
                    "[plugin] skipping {}: no init.lua found",
                    dir.display()
                ));
                continue;
            }

            crate::lsp::log_line(format!(
                "[plugin] loading {} from {}",
                name,
                init_path.display()
            ));

            if let Err(e) = self
                .lua
                .globals()
                .set("_TEDII_PLUGIN_DIR", dir.to_string_lossy().to_string())
            {
                crate::lsp::log_line(format!("[plugin] failed to set plugin dir: {}", e));
                continue;
            }

            let chunk = self.lua.load(init_path.as_path());
            let result = chunk.set_name(format!("plugin:{}", name)).exec();

            let _ = self.lua.globals().set("_TEDII_PLUGIN_DIR", mlua::Nil);

            match result {
                Ok(()) => {
                    crate::lsp::log_line(format!("[plugin] loaded {}", name));
                    let info = PluginInfo {
                        name,
                        path: dir.clone(),
                        error: None,
                    };
                    self.plugins.push(info.clone());
                    results.push(info);
                }
                Err(e) => {
                    let err_msg = e.to_string();
                    crate::lsp::log_line(format!("[plugin] failed to load {}: {}", name, err_msg));
                    results.push(PluginInfo {
                        name,
                        path: dir.clone(),
                        error: Some(err_msg),
                    });
                }
            }
        }

        results
    }

    #[allow(dead_code)]
    pub fn plugins(&self) -> &[PluginInfo] {
        &self.plugins
    }

    pub fn tick(&mut self) {
        let now = Instant::now();
        let mut i = 0;
        while i < self.pending.len() {
            let should_resume = self.pending[i].wake_at.is_none_or(|w| now >= w);
            if should_resume {
                let coroutine = self.pending.remove(i).unwrap();
                if coroutine.thread.resume::<mlua::Value>(()).is_ok()
                    && coroutine.thread.status() == mlua::ThreadStatus::Resumable
                {
                    self.pending.push_back(PendingCoroutine {
                        thread: coroutine.thread,
                        wake_at: coroutine.wake_at,
                    });
                    i += 1;
                }
            } else {
                i += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn new_runtime_creates_tedii_global() {
        let rt = PluginRuntime::new().unwrap();
        let tedii: mlua::Table = rt.lua.globals().get("tedii").unwrap();
        let version: String = tedii.get("version").unwrap();
        assert_eq!(version, "0.1.0");
    }

    #[test]
    fn load_from_empty_dir_returns_no_plugins() {
        let dir = tempfile::TempDir::new().unwrap();
        let mut rt = PluginRuntime::new().unwrap();
        let results = rt.load_plugins_from(dir.path().join("plugins"));
        assert!(results.is_empty());
        assert!(rt.plugins().is_empty());
    }

    #[test]
    fn load_valid_plugin() {
        let dir = tempfile::TempDir::new().unwrap();
        let plugins_dir = dir.path().join("plugins");
        let plugin_dir = plugins_dir.join("my_plugin");
        fs::create_dir_all(&plugin_dir).unwrap();
        fs::write(plugin_dir.join("init.lua"), "").unwrap();

        let mut rt = PluginRuntime::new().unwrap();
        let results = rt.load_plugins_from(plugins_dir);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "my_plugin");
        assert!(results[0].error.is_none());
    }

    #[test]
    fn load_plugin_with_error_reports_error() {
        let dir = tempfile::TempDir::new().unwrap();
        let plugins_dir = dir.path().join("plugins");
        let plugin_dir = plugins_dir.join("bad_plugin");
        fs::create_dir_all(&plugin_dir).unwrap();
        fs::write(plugin_dir.join("init.lua"), "error = nil + 1").unwrap();

        let mut rt = PluginRuntime::new().unwrap();
        let results = rt.load_plugins_from(plugins_dir);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "bad_plugin");
        assert!(results[0].error.is_some());
    }

    #[test]
    fn load_plugin_can_access_tedii_api() {
        let dir = tempfile::TempDir::new().unwrap();
        let plugins_dir = dir.path().join("plugins");
        let plugin_dir = plugins_dir.join("api_test");
        fs::create_dir_all(&plugin_dir).unwrap();
        fs::write(
            plugin_dir.join("init.lua"),
            r#"
tedii.log("hello from plugin")
assert(tedii.version == "0.1.0")
"#,
        )
        .unwrap();

        let mut rt = PluginRuntime::new().unwrap();
        let results = rt.load_plugins_from(plugins_dir);
        assert_eq!(results.len(), 1);
        assert!(results[0].error.is_none());
    }

    #[test]
    fn plugin_dir_returns_correct_path() {
        let dir = tempfile::TempDir::new().unwrap();
        let plugins_dir = dir.path().join("plugins");
        let plugin_dir = plugins_dir.join("path_test");
        fs::create_dir_all(&plugin_dir).unwrap();
        let plugin_path = plugin_dir.to_string_lossy().to_string();
        fs::write(
            plugin_dir.join("init.lua"),
            format!(
                r#"
local dir = tedii.plugin_dir()
assert(dir == "{}", "expected " .. dir)
"#,
                plugin_path
            ),
        )
        .unwrap();

        let mut rt = PluginRuntime::new().unwrap();
        let results = rt.load_plugins_from(plugins_dir);
        assert_eq!(results.len(), 1);
        assert!(results[0].error.is_none());
    }

    #[test]
    fn tick_does_not_panic() {
        let mut rt = PluginRuntime::new().unwrap();
        rt.tick();
    }

    #[test]
    fn load_multiple_plugins_in_order() {
        let dir = tempfile::TempDir::new().unwrap();
        let plugins_dir = dir.path().join("plugins");

        for name in &["beta", "alpha", "gamma"] {
            let pdir = plugins_dir.join(name);
            fs::create_dir_all(&pdir).unwrap();
            fs::write(pdir.join("init.lua"), "").unwrap();
        }

        let mut rt = PluginRuntime::new().unwrap();
        let results = rt.load_plugins_from(plugins_dir);
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].name, "alpha");
        assert_eq!(results[1].name, "beta");
        assert_eq!(results[2].name, "gamma");
    }

    #[test]
    fn load_skips_non_lua_dirs() {
        let dir = tempfile::TempDir::new().unwrap();
        let plugins_dir = dir.path().join("plugins");
        let plugin_dir = plugins_dir.join("real_plugin");
        fs::create_dir_all(&plugin_dir).unwrap();
        fs::write(plugin_dir.join("init.lua"), "").unwrap();
        // A directory without init.lua
        fs::create_dir_all(plugins_dir.join("no_init")).unwrap();
        // A file, not a directory
        fs::write(plugins_dir.join("not_a_dir.lua"), "").unwrap();

        let mut rt = PluginRuntime::new().unwrap();
        let results = rt.load_plugins_from(plugins_dir);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "real_plugin");
    }
}
