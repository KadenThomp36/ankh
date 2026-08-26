//! The Lua runtime: `defaults.lua` (embedded) then `~/.config/ankh/init.lua`.
//!
//! Lua never touches `App` directly. Native functions either read a
//! [`Snapshot`] the app refreshes before every callback, talk to the shared
//! [`Engine`], or push a [`Request`] the app drains afterwards. That keeps
//! re-entrancy impossible and the API surface explicit.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

use ankh_core::{Engine, ReviewCard};
use mlua::{Function, Lua, RegistryKey, Table, Value};

use crate::tui::app::Action;
use crate::tui::keys::{parse_seq, Keymap};

pub const DEFAULTS: &str = include_str!("defaults.lua");

/// Per-view key sequences removed with `ankh.keymap.del`.
pub type Deletions = HashMap<String, Vec<Vec<crate::tui::keys::Key>>>;

/// Options set through `ankh.setup`. Everything has a default in defaults.lua.
#[derive(Debug, Clone)]
pub struct Config {
    pub theme: String,
    /// "dark" | "light" | "auto"
    pub background: String,
    pub leader: String,
    pub timeoutlen_ms: u64,
    pub sync_on_launch: bool,
    pub sync_on_quit: bool,
    pub sync_media: bool,
    pub audio_autoplay: bool,
    pub show_timer: bool,
    /// Anything else, for plugins: `ankh.get("my.key")`.
    pub other: HashMap<String, String>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            theme: "tokyonight".into(),
            background: "auto".into(),
            leader: "<Space>".into(),
            timeoutlen_ms: 1000,
            sync_on_launch: true,
            sync_on_quit: true,
            sync_media: true,
            audio_autoplay: true,
            show_timer: true,
            other: HashMap::new(),
        }
    }
}

/// What a Lua callback asked the app to do.
#[derive(Debug, Clone)]
pub enum Request {
    Action(Action),
    Command(String),
    Notify(String),
    Error(String),
    Browse(String),
}

/// What Lua can see of the UI without touching it.
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub view: String,
    pub mode: String,
    pub card: Option<ReviewCard>,
    pub answer_shown: bool,
    pub deck: Option<String>,
}

struct State {
    config: Config,
    keymaps: HashMap<String, Keymap<Action>>,
    /// `ankh.keymap.del(view, lhs)` must also hide "global" bindings in that
    /// view, so deletions are remembered and applied after merging.
    deletions: Deletions,
    callbacks: Vec<RegistryKey>,
    commands: HashMap<String, (RegistryKey, Option<String>)>,
    handlers: HashMap<String, Vec<RegistryKey>>,
    requests: Vec<Request>,
    snapshot: Snapshot,
    errors: Vec<String>,
}

pub struct Runtime {
    lua: Lua,
    state: Rc<RefCell<State>>,
    engine: Rc<RefCell<Engine>>,
}

impl Runtime {
    /// Load defaults, then the user's init.lua if it exists. Errors are
    /// collected, never fatal: a broken config must not lock anyone out.
    pub fn new(engine: Rc<RefCell<Engine>>, init_lua: Option<PathBuf>) -> Runtime {
        let lua = Lua::new();
        // `require("plugin")` from ~/.config/ankh/plugins/.
        if let Some(dir) = init_lua.as_ref().and_then(|p| p.parent()).map(|p| p.join("plugins")) {
            let d = dir.display();
            let _ = lua
                .load(format!("package.path = '{d}/?.lua;{d}/?/init.lua;' .. package.path").replace('\\', "/"))
                .exec();
        }
        let state = Rc::new(RefCell::new(State {
            config: Config::default(),
            keymaps: HashMap::new(),
            deletions: HashMap::new(),
            callbacks: vec![],
            commands: HashMap::new(),
            handlers: HashMap::new(),
            requests: vec![],
            snapshot: Snapshot::default(),
            errors: vec![],
        }));
        let rt = Runtime { lua, state, engine };
        let native = match rt.native_table() {
            Ok(t) => t,
            Err(e) => {
                rt.state.borrow_mut().errors.push(format!("lua init: {e}"));
                return rt;
            }
        };
        if let Err(e) = rt.lua.load(DEFAULTS).set_name("defaults.lua").call::<()>(native) {
            rt.state.borrow_mut().errors.push(format!("defaults.lua: {e}"));
        }
        if let Some(path) = init_lua {
            if path.is_file() {
                match std::fs::read_to_string(&path) {
                    Ok(src) => {
                        if let Err(e) = rt.lua.load(&src).set_name(path.display().to_string()).exec() {
                            rt.state.borrow_mut().errors.push(format!("{e}"));
                        }
                    }
                    Err(e) => rt.state.borrow_mut().errors.push(format!("{}: {e}", path.display())),
                }
            }
        }
        rt
    }

    pub fn config(&self) -> Config {
        self.state.borrow().config.clone()
    }

    pub fn errors(&self) -> Vec<String> {
        std::mem::take(&mut self.state.borrow_mut().errors)
    }

    /// Keymaps by view name ("global", "decks", "review", "browser") and the
    /// per-view deletions to apply after merging global into each view.
    pub fn keymaps(&self) -> (HashMap<String, Keymap<Action>>, Deletions) {
        let st = self.state.borrow();
        (st.keymaps.clone(), st.deletions.clone())
    }

    pub fn has_command(&self, name: &str) -> bool {
        self.state.borrow().commands.contains_key(name)
    }

    pub fn command_names(&self) -> Vec<(String, Option<String>)> {
        let mut v: Vec<_> = self.state.borrow().commands.iter().map(|(k, (_, d))| (k.clone(), d.clone())).collect();
        v.sort();
        v
    }

    pub fn set_snapshot(&self, snap: Snapshot) {
        self.state.borrow_mut().snapshot = snap;
    }

    pub fn take_requests(&self) -> Vec<Request> {
        std::mem::take(&mut self.state.borrow_mut().requests)
    }

    fn report(&self, what: &str, e: mlua::Error) {
        self.state.borrow_mut().requests.push(Request::Error(format!("{what}: {e}")));
    }

    /// Run the Lua function bound to a key.
    pub fn call_action(&self, idx: usize) {
        let key = {
            let st = self.state.borrow();
            match st.callbacks.get(idx) {
                Some(k) => self.lua.registry_value::<Function>(k),
                None => return,
            }
        };
        match key {
            Ok(f) => {
                if let Err(e) = f.call::<()>(()) {
                    self.report("keymap", e);
                }
            }
            Err(e) => self.report("keymap", e),
        }
    }

    /// Run a user `:Command args`. Returns false if no such command.
    pub fn run_command(&self, name: &str, args: &str) -> bool {
        let f = {
            let st = self.state.borrow();
            match st.commands.get(name) {
                Some((k, _)) => self.lua.registry_value::<Function>(k),
                None => return false,
            }
        };
        match f {
            Ok(f) => {
                if let Err(e) = f.call::<()>(args) {
                    self.report(&format!(":{name}"), e);
                }
            }
            Err(e) => self.report(&format!(":{name}"), e),
        }
        true
    }

    /// Fire an event with a payload built by `payload`.
    pub fn emit(&self, event: &str, payload: impl FnOnce(&Lua) -> mlua::Result<Value>) {
        let keys: Vec<RegistryKey> = {
            let st = self.state.borrow();
            let Some(hs) = st.handlers.get(event) else { return };
            hs.iter()
                .filter_map(|k| self.lua.registry_value::<Function>(k).ok())
                .filter_map(|f| self.lua.create_registry_value(f).ok())
                .collect()
        };
        if keys.is_empty() {
            return;
        }
        let payload = match payload(&self.lua) {
            Ok(v) => v,
            Err(e) => {
                self.report(event, e);
                return;
            }
        };
        for k in keys {
            if let Ok(f) = self.lua.registry_value::<Function>(&k) {
                if let Err(e) = f.call::<()>(payload.clone()) {
                    self.report(&format!("on {event}"), e);
                }
            }
            let _ = self.lua.remove_registry_value(k);
        }
    }

    /// `:lua CODE` — evaluate as an expression first, then as a statement.
    pub fn eval(&self, code: &str) -> Result<String, String> {
        let expr = format!("return {code}");
        let r: mlua::Result<Value> = self.lua.load(&expr).set_name("=:lua").eval();
        let r = match r {
            Ok(v) => Ok(v),
            Err(_) => self.lua.load(code).set_name("=:lua").eval::<Value>(),
        };
        match r {
            Ok(Value::Nil) => Ok(String::new()),
            Ok(v) => Ok(lua_to_string(&v)),
            Err(e) => Err(e.to_string()),
        }
    }

    pub fn card_table(&self, card: &ReviewCard) -> mlua::Result<Table> {
        card_to_table(&self.lua, card)
    }

    // ----- native functions -------------------------------------------------

    fn native_table(&self) -> mlua::Result<Table> {
        let lua = &self.lua;
        let t = lua.create_table()?;
        let st = self.state.clone();
        let engine = self.engine.clone();

        // keymap_set(view, lhs, rhs, desc)
        {
            let st = st.clone();
            t.set(
                "keymap_set",
                lua.create_function(move |lua, (view, lhs, rhs, desc): (String, String, Value, Option<String>)| {
                    let seq = parse_seq(&lhs).map_err(mlua::Error::runtime)?;
                    let action = match rhs {
                        Value::String(s) => {
                            let s = s.to_str()?.to_string();
                            Action::from_name(&s).ok_or_else(|| {
                                mlua::Error::runtime(format!("unknown action {s:?}; see ankh.actions()"))
                            })?
                        }
                        Value::Function(f) => {
                            let key = lua.create_registry_value(f)?;
                            let mut st = st.borrow_mut();
                            st.callbacks.push(key);
                            Action::Lua(st.callbacks.len() - 1)
                        }
                        other => {
                            return Err(mlua::Error::runtime(format!(
                                "rhs must be an action name or function, got {}",
                                other.type_name()
                            )))
                        }
                    };
                    let mut st = st.borrow_mut();
                    let desc = desc.unwrap_or_else(|| lhs.clone());
                    st.keymaps.entry(view.to_ascii_lowercase()).or_default().bind_seq(seq, action, desc);
                    Ok(())
                })?,
            )?;
        }
        {
            let st = st.clone();
            t.set(
                "keymap_del",
                lua.create_function(move |_, (view, lhs): (String, String)| {
                    let seq = parse_seq(&lhs).map_err(mlua::Error::runtime)?;
                    let view = view.to_ascii_lowercase();
                    let mut st = st.borrow_mut();
                    if let Some(km) = st.keymaps.get_mut(&view) {
                        km.unbind(&seq);
                    }
                    st.deletions.entry(view).or_default().push(seq);
                    Ok(())
                })?,
            )?;
        }
        {
            let st = st.clone();
            t.set(
                "on",
                lua.create_function(move |lua, (event, f): (String, Function)| {
                    let key = lua.create_registry_value(f)?;
                    st.borrow_mut().handlers.entry(event).or_default().push(key);
                    Ok(())
                })?,
            )?;
        }
        {
            let st = st.clone();
            t.set(
                "command",
                lua.create_function(move |lua, (name, f, desc): (String, Function, Option<String>)| {
                    let key = lua.create_registry_value(f)?;
                    st.borrow_mut().commands.insert(name, (key, desc));
                    Ok(())
                })?,
            )?;
        }
        {
            let st = st.clone();
            t.set(
                "set_option",
                lua.create_function(move |_, (key, val): (String, Value)| {
                    let mut st = st.borrow_mut();
                    let c = &mut st.config;
                    let as_bool = |v: &Value| matches!(v, Value::Boolean(true));
                    let as_str = |v: &Value| lua_to_string(v);
                    match key.as_str() {
                        "theme" => c.theme = as_str(&val),
                        "background" => c.background = as_str(&val),
                        "leader" => c.leader = as_str(&val),
                        "timeoutlen" => c.timeoutlen_ms = as_str(&val).parse().unwrap_or(1000),
                        "sync.on_launch" => c.sync_on_launch = as_bool(&val),
                        "sync.on_quit" => c.sync_on_quit = as_bool(&val),
                        "sync.media" => c.sync_media = as_bool(&val),
                        "audio.autoplay" => c.audio_autoplay = as_bool(&val),
                        "review.show_timer" => c.show_timer = as_bool(&val),
                        _ => {
                            c.other.insert(key, as_str(&val));
                        }
                    }
                    Ok(())
                })?,
            )?;
        }
        {
            let st = st.clone();
            t.set(
                "get_option",
                lua.create_function(move |lua, key: String| {
                    let st = st.borrow();
                    let c = &st.config;
                    Ok(match key.as_str() {
                        "theme" => Value::String(lua.create_string(&c.theme)?),
                        "background" => Value::String(lua.create_string(&c.background)?),
                        "leader" => Value::String(lua.create_string(&c.leader)?),
                        "timeoutlen" => Value::Integer(c.timeoutlen_ms as i64),
                        "sync.on_launch" => Value::Boolean(c.sync_on_launch),
                        "sync.on_quit" => Value::Boolean(c.sync_on_quit),
                        "sync.media" => Value::Boolean(c.sync_media),
                        "audio.autoplay" => Value::Boolean(c.audio_autoplay),
                        "review.show_timer" => Value::Boolean(c.show_timer),
                        other => match c.other.get(other) {
                            Some(v) => Value::String(lua.create_string(v)?),
                            None => Value::Nil,
                        },
                    })
                })?,
            )?;
        }
        // Requests
        for (name, mk) in [
            ("notify", (|s: String| Request::Notify(s)) as fn(String) -> Request),
            ("error", |s| Request::Error(s)),
            ("cmd", |s| Request::Command(s)),
            ("browse", |s| Request::Browse(s)),
        ] {
            let st = st.clone();
            t.set(
                name,
                lua.create_function(move |_, s: String| {
                    st.borrow_mut().requests.push(mk(s));
                    Ok(())
                })?,
            )?;
        }
        {
            let st = st.clone();
            t.set(
                "action",
                lua.create_function(move |_, name: String| {
                    let a = Action::from_name(&name)
                        .ok_or_else(|| mlua::Error::runtime(format!("unknown action {name:?}")))?;
                    st.borrow_mut().requests.push(Request::Action(a));
                    Ok(())
                })?,
            )?;
        }
        t.set(
            "actions",
            lua.create_function(|lua, ()| {
                let t = lua.create_table()?;
                for (i, n) in Action::NAMES.iter().enumerate() {
                    t.set(i + 1, *n)?;
                }
                Ok(t)
            })?,
        )?;
        // Snapshot readers
        {
            let st = st.clone();
            t.set(
                "card_current",
                lua.create_function(move |lua, ()| {
                    let st = st.borrow();
                    match &st.snapshot.card {
                        Some(c) => {
                            let t = card_to_table(lua, c)?;
                            t.set("answer_shown", st.snapshot.answer_shown)?;
                            Ok(Value::Table(t))
                        }
                        None => Ok(Value::Nil),
                    }
                })?,
            )?;
        }
        {
            let st = st.clone();
            t.set("view", lua.create_function(move |_, ()| Ok(st.borrow().snapshot.view.clone()))?)?;
        }
        {
            let st = st.clone();
            t.set("mode", lua.create_function(move |_, ()| Ok(st.borrow().snapshot.mode.clone()))?)?;
        }
        {
            let st = st.clone();
            t.set("deck_current", lua.create_function(move |_, ()| Ok(st.borrow().snapshot.deck.clone()))?)?;
        }
        // Engine readers
        {
            let engine = engine.clone();
            t.set(
                "decks",
                lua.create_function(move |lua, ()| {
                    let tree = engine.borrow_mut().deck_tree().map_err(mlua::Error::external)?;
                    let out = lua.create_table()?;
                    for (i, d) in tree.all().into_iter().enumerate() {
                        let t = lua.create_table()?;
                        t.set("id", d.id.0)?;
                        t.set("name", d.name.clone())?;
                        t.set("full_name", d.full_name.clone())?;
                        t.set("level", d.level)?;
                        t.set("new", d.new)?;
                        t.set("learn", d.learn)?;
                        t.set("review", d.review)?;
                        t.set("total", d.total_with_children)?;
                        out.set(i + 1, t)?;
                    }
                    Ok(out)
                })?,
            )?;
        }
        {
            let engine = engine.clone();
            t.set(
                "search",
                lua.create_function(move |lua, query: String| {
                    let ids = engine
                        .borrow_mut()
                        .search(&query, ankh_core::SortBy::SortField, false)
                        .map_err(mlua::Error::external)?;
                    let out = lua.create_table()?;
                    for (i, id) in ids.into_iter().enumerate() {
                        out.set(i + 1, id)?;
                    }
                    Ok(out)
                })?,
            )?;
        }
        {
            let engine = engine.clone();
            t.set(
                "card_info",
                lua.create_function(move |lua, id: i64| {
                    let info = engine.borrow_mut().card_info(id).map_err(mlua::Error::external)?;
                    json_to_lua(lua, &serde_json::to_value(&info).unwrap())
                })?,
            )?;
        }
        {
            let engine = engine.clone();
            t.set(
                "note_get",
                lua.create_function(move |lua, id: i64| {
                    let n = engine.borrow_mut().note(id).map_err(mlua::Error::external)?;
                    let t = lua.create_table()?;
                    t.set("id", n.id)?;
                    t.set("notetype", n.notetype)?;
                    t.set("deck", n.deck)?;
                    t.set("tags", n.tags)?;
                    let fields = lua.create_table()?;
                    for (k, v) in n.fields {
                        fields.set(k, v)?;
                    }
                    t.set("fields", fields)?;
                    t.set("card_ids", n.card_ids)?;
                    Ok(t)
                })?,
            )?;
        }
        {
            let engine = engine.clone();
            let st = st.clone();
            t.set(
                "note_add",
                lua.create_function(move |_, spec: Table| {
                    let notetype: String = spec.get("notetype")?;
                    let deck: String = spec.get("deck")?;
                    let tags: Vec<String> = spec.get::<Option<Vec<String>>>("tags")?.unwrap_or_default();
                    let fields: Table = spec.get("fields")?;
                    let mut eng = engine.borrow_mut();
                    let names = eng
                        .field_names(&notetype)
                        .map_err(mlua::Error::external)?
                        .ok_or_else(|| mlua::Error::runtime(format!("unknown notetype {notetype:?}")))?;
                    let mut fs = Vec::new();
                    for n in names {
                        let v: Option<String> = fields.get(n.as_str())?;
                        fs.push((n, format!("{}{}", ankh_core::markdown::RAW_MARKER, v.unwrap_or_default())));
                    }
                    let doc = ankh_core::NoteDoc { id: None, notetype, deck, tags, fields: fs };
                    let (id, _) = eng.save_note(&doc).map_err(mlua::Error::external)?;
                    st.borrow_mut().requests.push(Request::Notify(format!("added note {id}")));
                    Ok(id)
                })?,
            )?;
        }
        t.set(
            "render",
            lua.create_function(|_, html: String| {
                Ok(ankh_render::render_html(&html, &ankh_render::Options::default()).plain_text())
            })?,
        )?;
        Ok(t)
    }
}

fn card_to_table(lua: &Lua, c: &ReviewCard) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.set("id", c.card_id)?;
    t.set("note_id", c.note_id)?;
    t.set("deck", c.deck_name.clone())?;
    t.set("notetype", c.notetype.clone())?;
    t.set("kind", format!("{:?}", c.kind).to_lowercase())?;
    t.set("flag", c.flag)?;
    t.set("tags", c.tags.clone())?;
    t.set("question_html", c.question_html.clone())?;
    t.set("answer_html", c.answer_html.clone())?;
    let opts = ankh_render::Options::default();
    t.set("question", ankh_render::render_html(&c.question_html, &opts).plain_text())?;
    t.set("answer", ankh_render::render_html(&c.answer_html, &opts).plain_text())?;
    t.set("buttons", c.buttons.to_vec())?;
    let counts = lua.create_table()?;
    counts.set("new", c.counts.new)?;
    counts.set("learn", c.counts.learn)?;
    counts.set("review", c.counts.review)?;
    t.set("counts", counts)?;
    Ok(t)
}

fn json_to_lua(lua: &Lua, v: &serde_json::Value) -> mlua::Result<Value> {
    Ok(match v {
        serde_json::Value::Null => Value::Nil,
        serde_json::Value::Bool(b) => Value::Boolean(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Integer(i)
            } else {
                Value::Number(n.as_f64().unwrap_or(0.0))
            }
        }
        serde_json::Value::String(s) => Value::String(lua.create_string(s)?),
        serde_json::Value::Array(a) => {
            let t = lua.create_table()?;
            for (i, x) in a.iter().enumerate() {
                t.set(i + 1, json_to_lua(lua, x)?)?;
            }
            Value::Table(t)
        }
        serde_json::Value::Object(o) => {
            let t = lua.create_table()?;
            for (k, x) in o {
                t.set(k.as_str(), json_to_lua(lua, x)?)?;
            }
            Value::Table(t)
        }
    })
}

fn lua_to_string(v: &Value) -> String {
    match v {
        Value::Nil => "nil".into(),
        Value::Boolean(b) => b.to_string(),
        Value::Integer(i) => i.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.to_str().map(|s| s.to_string()).unwrap_or_default(),
        Value::Table(t) => {
            let mut parts = Vec::new();
            for pair in t.clone().pairs::<Value, Value>().flatten() {
                parts.push(format!("{} = {}", lua_to_string(&pair.0), lua_to_string(&pair.1)));
                if parts.len() > 20 {
                    parts.push("…".into());
                    break;
                }
            }
            format!("{{ {} }}", parts.join(", "))
        }
        other => format!("<{}>", other.type_name()),
    }
}
