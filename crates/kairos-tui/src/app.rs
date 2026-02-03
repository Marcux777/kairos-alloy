use crate::logging::LogStore;
use crate::tasks::{StreamStatusSample, TaskEvent, TaskKind, TaskRunner, TradeSample};
use crossterm::event::{Event as CtEvent, KeyCode, KeyEvent, KeyModifiers};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

const MAX_SERIES_POINTS: usize = 600;
const MAX_TRADES: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewId {
    MainMenu,
    Setup,
    Backtest,
    Monitor,
    Reports,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BacktestTab {
    Validate,
    Backtest,
    Paper,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupFocus {
    Input,
    List,
}

pub struct TextInput {
    pub value: String,
    pub cursor: usize,
}

impl TextInput {
    pub fn new(value: String) -> Self {
        let cursor = value.len();
        Self { value, cursor }
    }

    pub fn insert_char(&mut self, ch: char) {
        self.value.insert(self.cursor, ch);
        self.cursor += 1;
    }

    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        self.cursor -= 1;
        self.value.remove(self.cursor);
    }

    pub fn delete(&mut self) {
        if self.cursor >= self.value.len() {
            return;
        }
        self.value.remove(self.cursor);
    }

    pub fn move_left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn move_right(&mut self) {
        self.cursor = (self.cursor + 1).min(self.value.len());
    }
}

pub struct RunStatus {
    pub running: bool,
    pub kind: Option<TaskKind>,
    pub started_at: Option<Instant>,
    pub last_result: Option<Result<String, String>>,
}

pub struct App {
    pub active_view: ViewId,
    pub menu_index: usize,

    pub config: Option<Arc<kairos_application::config::Config>>,
    pub config_toml: Option<String>,
    pub config_path: Option<PathBuf>,
    pub config_input: TextInput,
    pub setup_focus: SetupFocus,
    pub available_configs: Vec<PathBuf>,
    pub recent_config_count: usize,
    pub selected_config: usize,
    pub default_out_dir: PathBuf,

    pub backtest_tab: BacktestTab,
    pub validate_strict: bool,
    pub require_validate_before_run: bool,
    pub last_validate_ok: Option<bool>,
    pub paper_realtime: bool,
    pub stream_status: Option<StreamStatusSample>,

    pub logs: Arc<parking_lot::Mutex<LogStore>>,
    pub log_scroll: usize,

    pub price_series: VecDeque<(f64, f64)>,
    pub equity_series: VecDeque<(f64, f64)>,
    pub trades: VecDeque<TradeSample>,
    pub trade_scroll: usize,

    pub status: RunStatus,
    pub task_runner: TaskRunner,
    pub paused: bool,
    pub cancel_requested: bool,
    pub pause_blink: bool,
    tick_counter: u64,

    pub dirty: bool,
    spinner: usize,
    pub last_error: Option<String>,
    pub info_message: Option<String>,
    info_expires_at: Option<Instant>,
}

impl App {
    pub fn new(
        initial_config_path: Option<PathBuf>,
        default_out_dir: PathBuf,
        logs: Arc<parking_lot::Mutex<LogStore>>,
        task_runner: TaskRunner,
    ) -> Self {
        let config_path_str = initial_config_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        Self {
            active_view: ViewId::MainMenu,
            menu_index: 0,
            config: None,
            config_toml: None,
            config_path: initial_config_path,
            config_input: TextInput::new(config_path_str),
            setup_focus: SetupFocus::Input,
            available_configs: Vec::new(),
            recent_config_count: 0,
            selected_config: 0,
            default_out_dir,
            backtest_tab: BacktestTab::Validate,
            validate_strict: false,
            require_validate_before_run: false,
            last_validate_ok: None,
            paper_realtime: false,
            stream_status: None,
            logs,
            log_scroll: 0,
            price_series: VecDeque::new(),
            equity_series: VecDeque::new(),
            trades: VecDeque::new(),
            trade_scroll: 0,
            status: RunStatus {
                running: false,
                kind: None,
                started_at: None,
                last_result: None,
            },
            task_runner,
            paused: false,
            cancel_requested: false,
            pause_blink: true,
            tick_counter: 0,
            dirty: true,
            spinner: 0,
            last_error: None,
            info_message: None,
            info_expires_at: None,
        }
    }

    pub fn spawn_input_reader(&self, tx: tokio::sync::mpsc::UnboundedSender<TaskEvent>) {
        std::thread::spawn(move || {
            while let Ok(event) = crossterm::event::read() {
                let _ = tx.send(TaskEvent::Input(event));
            }
        });
    }

    pub fn on_tick(&mut self) {
        if self.status.running {
            self.tick_counter = self.tick_counter.wrapping_add(1);
            if !self.paused {
                self.spinner = (self.spinner + 1) % 4;
                self.dirty = true;
            } else if self.tick_counter.is_multiple_of(8) {
                self.pause_blink = !self.pause_blink;
                self.dirty = true;
            }
        }

        if let Some(until) = self.info_expires_at {
            if Instant::now() >= until {
                self.info_message = None;
                self.info_expires_at = None;
                self.dirty = true;
            }
        }
    }

    pub fn on_event(&mut self, event: TaskEvent) -> Result<bool, String> {
        match event {
            TaskEvent::Input(ct) => self.on_input(ct),
            TaskEvent::Progress(sample) => {
                self.price_series.push_back((sample.x, sample.price));
                self.equity_series.push_back((sample.x, sample.equity));
                while self.price_series.len() > MAX_SERIES_POINTS {
                    self.price_series.pop_front();
                }
                while self.equity_series.len() > MAX_SERIES_POINTS {
                    self.equity_series.pop_front();
                }
                for trade in sample.trades_in_bar {
                    self.trades.push_back(trade);
                }
                while self.trades.len() > MAX_TRADES {
                    self.trades.pop_front();
                }
                self.dirty = true;
                Ok(false)
            }
            TaskEvent::StreamStatus(status) => {
                self.stream_status = Some(status);
                self.dirty = true;
                Ok(false)
            }
            TaskEvent::TaskFinished(result) => {
                if self.status.kind == Some(TaskKind::Validate { strict: true })
                    || self.status.kind == Some(TaskKind::Validate { strict: false })
                {
                    self.last_validate_ok = Some(result.is_ok());
                }
                self.status.running = false;
                self.status.started_at = None;
                self.paused = false;
                self.cancel_requested = false;
                self.status.last_result = Some(match result {
                    Ok(ok) => Ok(ok),
                    Err(err) => {
                        if err.to_lowercase().contains("cancelled") {
                            Err("Cancelled".to_string())
                        } else {
                            Err(err)
                        }
                    }
                });
                self.stream_status = None;
                self.dirty = true;
                Ok(false)
            }
        }
    }

    fn on_input(&mut self, event: CtEvent) -> Result<bool, String> {
        match event {
            CtEvent::Key(key) => self.on_key(key),
            CtEvent::Resize(_, _) => {
                self.dirty = true;
                Ok(false)
            }
            _ => Ok(false),
        }
    }

    fn on_key(&mut self, key: KeyEvent) -> Result<bool, String> {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return Ok(true);
        }

        match self.active_view {
            ViewId::MainMenu => self.handle_menu_keys(key),
            ViewId::Setup => self.handle_setup_keys(key),
            ViewId::Backtest => self.handle_backtest_keys(key),
            ViewId::Monitor => self.handle_backtest_keys(key), // Share controls with Backtest
            ViewId::Reports => self.handle_simple_view_keys(key),
        }
    }

    fn handle_menu_keys(&mut self, key: KeyEvent) -> Result<bool, String> {
        match key.code {
            KeyCode::Char('q') => return Ok(true),
            KeyCode::Up => {
                self.menu_index = self.menu_index.saturating_sub(1);
                self.dirty = true;
            }
            KeyCode::Down => {
                self.menu_index = (self.menu_index + 1).min(3);
                self.dirty = true;
            }
            KeyCode::Enter => {
                self.active_view = match self.menu_index {
                    0 => {
                        self.refresh_available_configs();
                        self.selected_config = 0;
                        self.setup_focus = if self.available_configs.is_empty() {
                            SetupFocus::Input
                        } else {
                            SetupFocus::List
                        };
                        ViewId::Setup
                    }
                    1 => ViewId::Backtest,
                    2 => ViewId::Monitor,
                    3 => ViewId::Reports,
                    _ => ViewId::Setup,
                };
                self.dirty = true;
            }
            _ => {}
        }
        Ok(false)
    }

    fn handle_simple_view_keys(&mut self, key: KeyEvent) -> Result<bool, String> {
        match key.code {
            KeyCode::Esc => {
                self.active_view = ViewId::MainMenu;
                self.dirty = true;
            }
            KeyCode::Char('p') => {
                if self.status.running {
                    self.paused = self.task_runner.toggle_pause();
                    if self.paused {
                        self.pause_blink = true;
                    }
                    self.cancel_requested = false;
                    self.dirty = true;
                }
            }
            KeyCode::Char('x') => {
                if self.status.running {
                    self.task_runner.cancel_current();
                    self.cancel_requested = true;
                    self.paused = false;
                    self.status.last_result = Some(Err("Cancelled".to_string()));
                    self.dirty = true;
                }
            }
            KeyCode::Up => {
                if self.active_view == ViewId::Monitor {
                    let max = self.trades.len().saturating_sub(1);
                    self.trade_scroll = (self.trade_scroll + 1).min(max);
                    self.dirty = true;
                }
            }
            KeyCode::Down => {
                if self.active_view == ViewId::Monitor {
                    self.trade_scroll = self.trade_scroll.saturating_sub(1);
                    self.dirty = true;
                }
            }
            KeyCode::PageUp => {
                self.log_scroll = self.log_scroll.saturating_add(3);
                self.dirty = true;
            }
            KeyCode::PageDown => {
                self.log_scroll = self.log_scroll.saturating_sub(3);
                self.dirty = true;
            }
            _ => {}
        }
        Ok(false)
    }

    fn handle_setup_keys(&mut self, key: KeyEvent) -> Result<bool, String> {
        match key.code {
            KeyCode::Esc => {
                self.active_view = ViewId::MainMenu;
                self.dirty = true;
            }
            KeyCode::Char('g') | KeyCode::F(5) => {
                self.refresh_available_configs();
                self.info_message = Some("refreshed config list".to_string());
                self.info_expires_at = Some(Instant::now() + std::time::Duration::from_secs(2));
                self.dirty = true;
            }
            KeyCode::Tab => {
                self.setup_focus = match self.setup_focus {
                    SetupFocus::Input => SetupFocus::List,
                    SetupFocus::List => SetupFocus::Input,
                };
                self.dirty = true;
            }
            KeyCode::Char('i') => {
                self.setup_focus = SetupFocus::Input;
                self.dirty = true;
            }
            KeyCode::Char('l') => {
                self.setup_focus = SetupFocus::List;
                self.dirty = true;
            }
            KeyCode::Enter => {
                match self.setup_focus {
                    SetupFocus::Input => self.try_load_config(),
                    SetupFocus::List => self.try_load_selected_config(),
                }
                self.dirty = true;
            }
            KeyCode::Up => {
                if self.setup_focus == SetupFocus::List {
                    self.selected_config = self.selected_config.saturating_sub(1);
                    self.dirty = true;
                }
            }
            KeyCode::Down => {
                if self.setup_focus == SetupFocus::List {
                    let max = self.available_configs.len().saturating_sub(1);
                    self.selected_config = (self.selected_config + 1).min(max);
                    self.dirty = true;
                }
            }
            KeyCode::Backspace => {
                if self.setup_focus == SetupFocus::Input {
                    self.config_input.backspace();
                    self.dirty = true;
                }
            }
            KeyCode::Delete => {
                if self.setup_focus == SetupFocus::Input {
                    self.config_input.delete();
                    self.dirty = true;
                }
            }
            KeyCode::Left => {
                if self.setup_focus == SetupFocus::Input {
                    self.config_input.move_left();
                    self.dirty = true;
                }
            }
            KeyCode::Right => {
                if self.setup_focus == SetupFocus::Input {
                    self.config_input.move_right();
                    self.dirty = true;
                }
            }
            KeyCode::Char(ch) => {
                if !key.modifiers.contains(KeyModifiers::CONTROL)
                    && self.setup_focus == SetupFocus::Input
                {
                    self.config_input.insert_char(ch);
                    self.dirty = true;
                }
            }
            _ => {}
        }
        Ok(false)
    }

    fn handle_backtest_keys(&mut self, key: KeyEvent) -> Result<bool, String> {
        match key.code {
            KeyCode::Esc => {
                self.active_view = ViewId::MainMenu;
                self.dirty = true;
            }
            KeyCode::Left => {
                self.backtest_tab = match self.backtest_tab {
                    BacktestTab::Validate => BacktestTab::Validate,
                    BacktestTab::Backtest => BacktestTab::Validate,
                    BacktestTab::Paper => BacktestTab::Backtest,
                };
                self.dirty = true;
            }
            KeyCode::Right => {
                self.backtest_tab = match self.backtest_tab {
                    BacktestTab::Validate => BacktestTab::Backtest,
                    BacktestTab::Backtest => BacktestTab::Paper,
                    BacktestTab::Paper => BacktestTab::Paper,
                };
                self.dirty = true;
            }
            KeyCode::Char('s') => {
                if self.backtest_tab == BacktestTab::Validate {
                    self.validate_strict = !self.validate_strict;
                    self.dirty = true;
                }
            }
            KeyCode::Char('v') => {
                self.require_validate_before_run = !self.require_validate_before_run;
                self.dirty = true;
            }
            KeyCode::Char('t') => {
                if self.backtest_tab == BacktestTab::Paper && !self.status.running {
                    self.paper_realtime = !self.paper_realtime;
                    self.info_message = Some(if self.paper_realtime {
                        "paper mode: realtime on".to_string()
                    } else {
                        "paper mode: replay on".to_string()
                    });
                    self.info_expires_at = Some(Instant::now() + std::time::Duration::from_secs(2));
                    self.dirty = true;
                }
            }
            KeyCode::Char('r') => {
                self.start_selected_task()?;
                self.dirty = true;
            }
            KeyCode::Char('p') => {
                if self.status.running {
                    self.paused = self.task_runner.toggle_pause();
                    if self.paused {
                        self.pause_blink = true;
                    }
                    self.cancel_requested = false;
                    self.dirty = true;
                }
            }
            KeyCode::Char('x') => {
                if self.status.running {
                    self.task_runner.cancel_current();
                    self.cancel_requested = true;
                    self.paused = false;
                    self.status.last_result = Some(Err("Cancelled".to_string()));
                    self.dirty = true;
                }
            }
            KeyCode::Up => {
                if self.active_view == ViewId::Monitor {
                    let max = self.trades.len().saturating_sub(1);
                    self.trade_scroll = (self.trade_scroll + 1).min(max);
                    self.dirty = true;
                }
            }
            KeyCode::Down => {
                if self.active_view == ViewId::Monitor {
                    self.trade_scroll = self.trade_scroll.saturating_sub(1);
                    self.dirty = true;
                }
            }
            KeyCode::PageUp => {
                self.log_scroll = self.log_scroll.saturating_add(3);
                self.dirty = true;
            }
            KeyCode::PageDown => {
                self.log_scroll = self.log_scroll.saturating_sub(3);
                self.dirty = true;
            }
            _ => {}
        }
        Ok(false)
    }

    pub fn try_load_config(&mut self) {
        let raw = self.config_input.value.trim().to_string();
        if raw.is_empty() {
            self.last_error = Some("config path is empty".to_string());
            return;
        }

        let path = PathBuf::from(raw.clone());
        match kairos_application::config::load_config_with_source(&path) {
            Ok((cfg, source)) => {
                self.config_path = Some(path);
                self.config = Some(Arc::new(cfg));
                self.config_toml = Some(source);
                self.last_error = None;
                self.record_recent_config(&raw);
                self.refresh_available_configs();
                tracing::info!(config_path = %raw, "config loaded");
            }
            Err(err) => {
                self.last_error = Some(err);
            }
        }
    }

    fn try_load_selected_config(&mut self) {
        let Some(path) = self.available_configs.get(self.selected_config).cloned() else {
            self.last_error = Some("no configs found under configs/ or recents".to_string());
            return;
        };
        self.config_input = TextInput::new(path.display().to_string());
        self.try_load_config();
    }

    fn refresh_available_configs(&mut self) {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let configs_dir = cwd.join("configs");
        let mut configs: Vec<PathBuf> = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&configs_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path
                    .extension()
                    .and_then(|s| s.to_str())
                    .map(|s| s.eq_ignore_ascii_case("toml"))
                    .unwrap_or(false)
                {
                    configs.push(std::fs::canonicalize(&path).unwrap_or(path));
                }
            }
        }
        configs.sort();

        let recents = load_recent_configs()
            .unwrap_or_default()
            .into_iter()
            .filter(|p| p.exists())
            .collect::<Vec<_>>();

        let (merged, recent_count) = merge_recents_and_configs(&recents, &configs);
        self.recent_config_count = recent_count;
        self.available_configs = merged;
        if self.selected_config >= self.available_configs.len() {
            self.selected_config = self.available_configs.len().saturating_sub(1);
        }
    }

    fn record_recent_config(&self, raw_path: &str) {
        let path = PathBuf::from(raw_path);
        let abs = std::fs::canonicalize(&path).unwrap_or(path);
        let mut recents = load_recent_configs().unwrap_or_default();
        recents.retain(|p| p != &abs);
        recents.insert(0, abs);
        recents.truncate(10);
        let _ = store_recent_configs(&recents);
    }

    fn start_selected_task(&mut self) -> Result<(), String> {
        if self.status.running {
            return Ok(());
        }

        let Some(cfg) = self.config.as_ref().map(Arc::clone) else {
            self.last_error = Some("load a config first (Setup)".to_string());
            return Ok(());
        };
        let cfg_toml = self.config_toml.clone().unwrap_or_default();

        let kind = match self.backtest_tab {
            BacktestTab::Validate => TaskKind::Validate {
                strict: self.validate_strict,
            },
            BacktestTab::Backtest => TaskKind::Backtest,
            BacktestTab::Paper => {
                if self.paper_realtime {
                    TaskKind::PaperRealtime
                } else {
                    TaskKind::Paper
                }
            }
        };

        if self.require_validate_before_run
            && matches!(
                kind,
                TaskKind::Backtest | TaskKind::Paper | TaskKind::PaperRealtime
            )
            && self.last_validate_ok != Some(true)
        {
            self.last_error =
                Some("run Validate first (press ←/→ to tab Validate, then r)".to_string());
            return Ok(());
        }

        self.status.running = true;
        self.paused = false;
        self.cancel_requested = false;
        self.pause_blink = true;
        self.tick_counter = 0;
        self.status.kind = Some(kind);
        self.status.started_at = Some(Instant::now());
        self.status.last_result = None;

        self.price_series.clear();
        self.equity_series.clear();
        self.trades.clear();
        self.trade_scroll = 0;
        self.paused = false;
        self.stream_status = None;
        if matches!(
            kind,
            TaskKind::Backtest | TaskKind::Paper | TaskKind::PaperRealtime
        ) {
            self.active_view = ViewId::Monitor;
        }

        self.task_runner.start(kind, cfg, cfg_toml);
        Ok(())
    }

    pub fn spinner_char(&self) -> char {
        match self.spinner {
            0 => '|',
            1 => '/',
            2 => '-',
            _ => '\\',
        }
    }
}

fn recent_store_path() -> Option<PathBuf> {
    let override_dir = std::env::var("KAIROS_TUI_CONFIG_HOME").ok();
    let xdg = std::env::var("XDG_CONFIG_HOME").ok();
    let home = std::env::var("HOME").ok();
    recent_store_path_from(override_dir.as_deref(), xdg.as_deref(), home.as_deref())
}

fn recent_store_path_from(
    config_home_override: Option<&str>,
    xdg_config_home: Option<&str>,
    home: Option<&str>,
) -> Option<PathBuf> {
    if let Some(dir) = config_home_override {
        if !dir.trim().is_empty() {
            return Some(PathBuf::from(dir).join("recent_configs.json"));
        }
    }
    if let Some(xdg) = xdg_config_home {
        if !xdg.trim().is_empty() {
            return Some(
                PathBuf::from(xdg)
                    .join("kairos-alloy")
                    .join("recent_configs.json"),
            );
        }
    }
    if let Some(home) = home {
        if !home.trim().is_empty() {
            return Some(
                PathBuf::from(home)
                    .join(".config")
                    .join("kairos-alloy")
                    .join("recent_configs.json"),
            );
        }
    }
    None
}

fn load_recent_configs() -> Result<Vec<PathBuf>, String> {
    let Some(path) = recent_store_path() else {
        return Ok(Vec::new());
    };
    load_recent_configs_from(&path)
}

fn store_recent_configs(paths: &[PathBuf]) -> Result<(), String> {
    let Some(path) = recent_store_path() else {
        return Ok(());
    };
    store_recent_configs_to(&path, paths)
}

fn load_recent_configs_from(path: &std::path::Path) -> Result<Vec<PathBuf>, String> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let contents = std::fs::read_to_string(path)
        .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    let raw: Vec<String> = serde_json::from_str(&contents)
        .map_err(|err| format!("failed to parse {}: {err}", path.display()))?;
    Ok(raw.into_iter().map(PathBuf::from).collect())
}

fn store_recent_configs_to(path: &std::path::Path, paths: &[PathBuf]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
    }
    let raw: Vec<String> = paths.iter().map(|p| p.display().to_string()).collect();
    let json = serde_json::to_string_pretty(&raw)
        .map_err(|err| format!("failed to serialize recent configs: {err}"))?;
    std::fs::write(path, json).map_err(|err| format!("failed to write {}: {err}", path.display()))
}

fn merge_recents_and_configs(recents: &[PathBuf], configs: &[PathBuf]) -> (Vec<PathBuf>, usize) {
    let mut merged: Vec<PathBuf> = Vec::new();
    for p in recents {
        if !merged.contains(p) {
            merged.push(p.clone());
        }
    }
    let recent_count = merged.len();
    for p in configs {
        if !merged.contains(p) {
            merged.push(p.clone());
        }
    }
    (merged, recent_count)
}

#[cfg(test)]
mod tests {
    use super::{
        load_recent_configs_from, merge_recents_and_configs, recent_store_path_from,
        store_recent_configs_to,
    };
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn merge_recents_and_configs_places_recents_first_and_dedupes() {
        let recents = vec![PathBuf::from("/a.toml"), PathBuf::from("/b.toml")];
        let configs = vec![PathBuf::from("/b.toml"), PathBuf::from("/c.toml")];
        let (merged, recent_count) = merge_recents_and_configs(&recents, &configs);
        assert_eq!(recent_count, 2);
        assert_eq!(
            merged,
            vec![
                PathBuf::from("/a.toml"),
                PathBuf::from("/b.toml"),
                PathBuf::from("/c.toml")
            ]
        );
    }

    #[test]
    fn recent_store_path_prefers_override_then_xdg_then_home() {
        let path = recent_store_path_from(Some("/tmp/kairos"), Some("/xdg"), Some("/home/u"))
            .expect("path");
        assert_eq!(
            path,
            PathBuf::from("/tmp/kairos").join("recent_configs.json")
        );

        let path = recent_store_path_from(None, Some("/xdg"), Some("/home/u")).expect("path");
        assert_eq!(
            path,
            PathBuf::from("/xdg")
                .join("kairos-alloy")
                .join("recent_configs.json")
        );

        let path = recent_store_path_from(None, None, Some("/home/u")).expect("path");
        assert_eq!(
            path,
            PathBuf::from("/home/u")
                .join(".config")
                .join("kairos-alloy")
                .join("recent_configs.json")
        );
    }

    #[test]
    fn store_and_load_recent_configs_roundtrip() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let path = std::env::temp_dir().join(format!("kairos_recent_{unique}.json"));
        let list = vec![PathBuf::from("/a.toml"), PathBuf::from("/b.toml")];

        store_recent_configs_to(&path, &list).expect("store");
        let loaded = load_recent_configs_from(&path).expect("load");
        assert_eq!(loaded, list);

        let _ = std::fs::remove_file(&path);
    }
}
