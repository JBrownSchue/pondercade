//! Claude Code integration.
//!
//! Claude Code runs `pondercade hook` from its hooks (see `install`). The hook reads the event
//! JSON from stdin, appends a one-line event to `events.jsonl` in Pondercade's data folder and
//! exits immediately; it never prints anything (stdout of a `UserPromptSubmit` hook would be
//! added to Claude's context). The running app polls that file and shows a toast.
//!
//! Files in `<data dir>/claude/`:
//! - `events.jsonl`  — appended by the hook, read incrementally by the app
//! - `heartbeat`     — unix time written by the app every few seconds ("is the app open?")
//! - `launching`     — unix time of the last auto-launch, so parallel hooks don't open two windows
//! - `no_autolaunch` — exists when the user turned auto-launch off
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Marker used to recognise our own entries in Claude's settings.json.
const HOOK_ARG: &str = "hook";
const HOOK_EVENTS: [&str; 3] = ["UserPromptSubmit", "Stop", "Notification"];
const EVENTS_MAX_BYTES: u64 = 256 * 1024;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// The user sent a prompt; Claude is working.
    Working,
    /// Claude finished its turn.
    Done,
    /// Claude is waiting for the user (permission prompt, question, idle).
    Input,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Event {
    pub kind: Kind,
    /// Project folder name (last component of the session's cwd).
    #[serde(default)]
    pub project: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub session: String,
    /// Unix time the hook ran.
    #[serde(default)]
    pub at: u64,
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

pub fn dir() -> Option<PathBuf> {
    eframe::storage_dir(crate::APP_NAME).map(|d| d.join("claude"))
}

fn read_secs(name: &str) -> Option<u64> {
    fs::read_to_string(dir()?.join(name)).ok()?.trim().parse().ok()
}

fn write_secs(name: &str) {
    if let Some(d) = dir() {
        let _ = fs::create_dir_all(&d);
        let _ = fs::write(d.join(name), now_secs().to_string());
    }
}

// --- hook side ------------------------------------------------------------------------------

/// Turns the JSON Claude Code sends on stdin into an event (None = ignore).
pub fn parse_hook_input(input: &str) -> Option<Event> {
    let v: Value = serde_json::from_str(input).ok()?;
    let s = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or_default().to_owned();
    let kind = match s("hook_event_name").as_str() {
        "UserPromptSubmit" => Kind::Working,
        // Stop fires again after a Stop hook made Claude continue; only the final stop matters.
        "Stop" if v.get("stop_hook_active").and_then(Value::as_bool) != Some(true) => Kind::Done,
        // Only notifications that mean "Claude is waiting for you" (not e.g. auth_success).
        "Notification"
            if matches!(
                s("notification_type").as_str(),
                "" | "permission_prompt" | "idle_prompt" | "elicitation_dialog" | "elicitation_url_dialog" | "agent_needs_input"
            ) =>
        {
            Kind::Input
        }
        _ => return None,
    };
    let cwd = s("cwd");
    let project = cwd.trim_end_matches(['/', '\\']).rsplit(['/', '\\']).next().unwrap_or_default().to_owned();
    let message = if kind == Kind::Input { s("message") } else { String::new() };
    Some(Event { kind, project, message, session: s("session_id"), at: now_secs() })
}

fn append_event(e: &Event) -> std::io::Result<()> {
    let Some(d) = dir() else { return Ok(()) };
    fs::create_dir_all(&d)?;
    let path = d.join("events.jsonl");
    // Keep the file small: start over once it grows (the app handles the shrink).
    if fs::metadata(&path).is_ok_and(|m| m.len() > EVENTS_MAX_BYTES) {
        let _ = fs::remove_file(&path);
    }
    let mut f = fs::OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(f, "{}", serde_json::to_string(e).unwrap_or_default())
}

pub fn app_is_open() -> bool {
    read_secs("heartbeat").is_some_and(|t| now_secs().saturating_sub(t) <= 6)
}

pub fn autolaunch_enabled() -> bool {
    !dir().is_some_and(|d| d.join("no_autolaunch").exists())
}

pub fn set_autolaunch(on: bool) {
    let Some(d) = dir() else { return };
    let _ = fs::create_dir_all(&d);
    let path = d.join("no_autolaunch");
    if on {
        let _ = fs::remove_file(path);
    } else {
        let _ = fs::write(path, "");
    }
}

/// Starts the app detached from the hook (no inherited stdio, so Claude Code doesn't wait for it).
fn launch_app() {
    if read_secs("launching").is_some_and(|t| now_secs().saturating_sub(t) < 15) {
        return;
    }
    write_secs("launching");
    let Ok(exe) = std::env::current_exe() else { return };
    let mut cmd = std::process::Command::new(exe);
    cmd.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    }
    let _ = cmd.spawn();
}

/// Entry point for `pondercade hook`. Always exits successfully and silently.
pub fn run_hook() {
    let mut input = String::new();
    let _ = std::io::stdin().read_to_string(&mut input);
    let Some(e) = parse_hook_input(&input) else { return };
    let _ = append_event(&e);
    if e.kind == Kind::Working && autolaunch_enabled() && !app_is_open() {
        launch_app();
    }
}

// --- app side -------------------------------------------------------------------------------

/// Reads new events appended since the last poll.
pub struct Watcher {
    offset: u64,
    /// Events older than this (unix secs) are history and not shown.
    since: u64,
    last_poll: f64,
    last_beat: f64,
}

impl Watcher {
    pub fn new() -> Self {
        // Replay only the last few seconds: that covers the hook that auto-opened the app and
        // anything Claude reported while the window was still starting up.
        write_secs("heartbeat");
        Self { offset: 0, since: now_secs().saturating_sub(10), last_poll: -1.0, last_beat: 0.0 }
    }

    /// Call every frame; returns new events (at most every half second).
    pub fn poll(&mut self, now: f64) -> Vec<Event> {
        if now - self.last_beat >= 2.0 {
            self.last_beat = now;
            write_secs("heartbeat");
        }
        if now - self.last_poll < 0.5 {
            return Vec::new();
        }
        self.last_poll = now;
        let Some(path) = dir().map(|d| d.join("events.jsonl")) else { return Vec::new() };
        let Ok(mut f) = fs::File::open(path) else {
            self.offset = 0;
            return Vec::new();
        };
        let len = f.metadata().map_or(0, |m| m.len());
        if len < self.offset {
            self.offset = 0; // file was truncated by the hook
        }
        if len == self.offset || f.seek(SeekFrom::Start(self.offset)).is_err() {
            return Vec::new();
        }
        let mut buf = String::new();
        if f.read_to_string(&mut buf).is_err() {
            return Vec::new();
        }
        // Only consume complete lines; a half-written line is picked up next time.
        let complete = buf.rfind('\n').map_or(0, |i| i + 1);
        self.offset += complete as u64;
        buf[..complete].lines().filter_map(|l| serde_json::from_str::<Event>(l).ok()).filter(|e| e.at >= self.since).collect()
    }
}

/// Clear the heartbeat on exit so the next prompt relaunches the app straight away.
pub fn app_closed() {
    if let Some(d) = dir() {
        let _ = fs::remove_file(d.join("heartbeat"));
    }
}

// --- installing into ~/.claude/settings.json --------------------------------------------------

fn settings_path() -> Option<PathBuf> {
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
    Some(PathBuf::from(home).join(".claude").join("settings.json"))
}

/// Path of this executable. Hooks use Claude Code's exec form (`command` + `args`), which runs
/// the program directly without a shell, so spaces in the path need no quoting.
fn hook_command() -> Option<String> {
    Some(std::env::current_exe().ok()?.to_string_lossy().into_owned())
}

fn hook_entry(command: &str) -> Value {
    // async: fire-and-forget, so the hook never delays Claude.
    json!({ "type": "command", "command": command, "args": [HOOK_ARG], "async": true })
}

fn is_ours(hook: &Value) -> bool {
    let cmd = hook.get("command").and_then(Value::as_str).unwrap_or_default().to_ascii_lowercase();
    let args_hook = hook.get("args").and_then(Value::as_array).is_some_and(|a| a.len() == 1 && a[0] == HOOK_ARG);
    cmd.contains("pondercade") && (args_hook || cmd.trim_end().ends_with(" hook"))
}

/// Removes our entries from a parsed settings object; returns true if anything changed.
fn strip_ours(settings: &mut Value) -> bool {
    let Some(hooks) = settings.get_mut("hooks").and_then(Value::as_object_mut) else { return false };
    let mut changed = false;
    for ev in HOOK_EVENTS {
        let Some(groups) = hooks.get_mut(ev).and_then(Value::as_array_mut) else { continue };
        for g in groups.iter_mut() {
            if let Some(list) = g.get_mut("hooks").and_then(Value::as_array_mut) {
                let before = list.len();
                list.retain(|h| !is_ours(h));
                changed |= list.len() != before;
            }
        }
        groups.retain(|g| g.get("hooks").and_then(Value::as_array).is_none_or(|l| !l.is_empty()));
        if groups.is_empty() {
            hooks.remove(ev);
        }
    }
    if hooks.is_empty() {
        settings.as_object_mut().map(|o| o.remove("hooks"));
    }
    changed
}

fn add_ours(settings: &mut Value, command: &str) {
    if !settings.is_object() {
        *settings = json!({});
    }
    let obj = settings.as_object_mut().unwrap();
    let hooks = obj.entry("hooks").or_insert_with(|| json!({}));
    if !hooks.is_object() {
        *hooks = json!({});
    }
    let hooks = hooks.as_object_mut().unwrap();
    for ev in HOOK_EVENTS {
        let group = json!({ "hooks": [hook_entry(command)] });
        let list = hooks.entry(ev).or_insert_with(|| json!([]));
        if !list.is_array() {
            *list = json!([]);
        }
        list.as_array_mut().unwrap().push(group);
    }
}

fn load_settings() -> Result<(PathBuf, Value), String> {
    let path = settings_path().ok_or("Couldn't find your home folder")?;
    let v = match fs::read_to_string(&path) {
        Ok(s) if !s.trim().is_empty() => serde_json::from_str(&s).map_err(|e| format!("Couldn't read {}: {e}", path.display()))?,
        _ => json!({}),
    };
    Ok((path, v))
}

fn save_settings(path: &PathBuf, v: &Value) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    // Keep a backup of the user's file the first time we touch it.
    let backup = path.with_extension("json.pondercade-backup");
    if path.exists() && !backup.exists() {
        let _ = fs::copy(path, &backup);
    }
    let text = serde_json::to_string_pretty(v).map_err(|e| e.to_string())?;
    fs::write(path, text + "\n").map_err(|e| format!("Couldn't write {}: {e}", path.display()))
}

/// Whether our hooks are present, and whether they point at this executable.
pub fn installed() -> (bool, bool) {
    let Ok((_, v)) = load_settings() else { return (false, false) };
    let cmd = hook_command().unwrap_or_default();
    let mut any = false;
    let mut current = false;
    if let Some(hooks) = v.get("hooks").and_then(Value::as_object) {
        for g in hooks.values().filter_map(Value::as_array).flatten() {
            for h in g.get("hooks").and_then(Value::as_array).into_iter().flatten() {
                if is_ours(h) {
                    any = true;
                    current |= h.get("command").and_then(Value::as_str) == Some(cmd.as_str());
                }
            }
        }
    }
    (any, current)
}

/// Installs (or re-points) the hooks. Other settings and hooks are left untouched.
pub fn install() -> Result<(), String> {
    let (path, mut v) = load_settings()?;
    let cmd = hook_command().ok_or("Couldn't find Pondercade's own path")?;
    strip_ours(&mut v);
    add_ours(&mut v, &cmd);
    save_settings(&path, &v)
}

pub fn uninstall() -> Result<(), String> {
    let (path, mut v) = load_settings()?;
    if strip_ours(&mut v) { save_settings(&path, &v) } else { Ok(()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hook_events() {
        let e = parse_hook_input(r#"{"hook_event_name":"UserPromptSubmit","cwd":"C:\\git\\pondercade","session_id":"s1","prompt":"hi"}"#).unwrap();
        assert_eq!((e.kind, e.project.as_str()), (Kind::Working, "pondercade"));
        let e = parse_hook_input(r#"{"hook_event_name":"Notification","cwd":"/home/a/proj/","message":"Claude needs your permission to use Bash"}"#).unwrap();
        assert_eq!((e.kind, e.project.as_str()), (Kind::Input, "proj"));
        assert!(e.message.contains("permission"));
        assert_eq!(parse_hook_input(r#"{"hook_event_name":"Stop","stop_hook_active":false}"#).unwrap().kind, Kind::Done);
        assert!(parse_hook_input(r#"{"hook_event_name":"Stop","stop_hook_active":true}"#).is_none());
        assert!(parse_hook_input(r#"{"hook_event_name":"PreToolUse"}"#).is_none());
        assert!(parse_hook_input(r#"{"hook_event_name":"Notification","notification_type":"auth_success"}"#).is_none());
        let e = parse_hook_input(r#"{"hook_event_name":"Notification","notification_type":"idle_prompt","message":"waiting"}"#);
        assert_eq!(e.unwrap().kind, Kind::Input);
        assert!(parse_hook_input("not json").is_none());
    }

    #[test]
    fn install_merges_and_uninstall_restores() {
        let original = json!({
            "model": "opus",
            "hooks": { "Stop": [ { "hooks": [ { "type": "command", "command": "say done" } ] } ] }
        });
        let mut v = original.clone();
        let exe = r"C:\Program Files\Pondercade\pondercade.exe";
        add_ours(&mut v, exe);
        assert_eq!(v["hooks"]["Stop"].as_array().unwrap().len(), 2);
        let ours = &v["hooks"]["Notification"][0]["hooks"][0];
        assert_eq!((ours["command"].as_str(), &ours["args"], &ours["async"]), (Some(exe), &json!(["hook"]), &json!(true)));
        assert!(is_ours(&v["hooks"]["Stop"][1]["hooks"][0]));
        assert!(!is_ours(&v["hooks"]["Stop"][0]["hooks"][0]), "the user's own hook must not be touched");
        assert_eq!(v["model"], "opus");
        // Key order of the user's file is preserved.
        assert_eq!(v.as_object().unwrap().keys().next().unwrap(), "model");
        // Re-installing doesn't duplicate.
        strip_ours(&mut v);
        add_ours(&mut v, "/usr/bin/pondercade");
        assert_eq!(v["hooks"]["UserPromptSubmit"].as_array().unwrap().len(), 1);
        assert!(strip_ours(&mut v));
        assert_eq!(v, original);
    }
}
