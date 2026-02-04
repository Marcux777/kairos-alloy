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
pub enum ReportsMode {
    Runs,
    AnalyzerList,
    AnalyzerDetail,
}

#[derive(Debug, Clone)]
pub struct ReportsRun {
    pub run_id: String,
    pub line: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupFocus {
    Input,
    QuickEdit,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuickEditField {
    RunId,
    Symbol,
    Timeframe,
    InitialCapital,
}

impl QuickEditField {
    fn next(self) -> Self {
        match self {
            Self::RunId => Self::Symbol,
            Self::Symbol => Self::Timeframe,
            Self::Timeframe => Self::InitialCapital,
            Self::InitialCapital => Self::RunId,
        }
    }

    fn prev(self) -> Self {
        match self {
            Self::RunId => Self::InitialCapital,
            Self::Symbol => Self::RunId,
            Self::Timeframe => Self::Symbol,
            Self::InitialCapital => Self::Timeframe,
        }
    }
}

pub struct QuickEditState {
    pub selected: QuickEditField,
    pub run_id: TextInput,
    pub symbol: TextInput,
    pub timeframe: TextInput,
    pub initial_capital: TextInput,
}

impl QuickEditState {
    pub fn new() -> Self {
        Self {
            selected: QuickEditField::RunId,
            run_id: TextInput::new(String::new()),
            symbol: TextInput::new(String::new()),
            timeframe: TextInput::new(String::new()),
            initial_capital: TextInput::new(String::new()),
        }
    }

    pub fn sync_from_config(&mut self, cfg: &kairos_application::config::Config) {
        self.run_id = TextInput::new(cfg.run.run_id.clone());
        self.symbol = TextInput::new(cfg.run.symbol.clone());
        self.timeframe = TextInput::new(cfg.run.timeframe.clone());
        self.initial_capital = TextInput::new(format!("{}", cfg.run.initial_capital));
    }

    fn selected_input_mut(&mut self) -> &mut TextInput {
        match self.selected {
            QuickEditField::RunId => &mut self.run_id,
            QuickEditField::Symbol => &mut self.symbol,
            QuickEditField::Timeframe => &mut self.timeframe,
            QuickEditField::InitialCapital => &mut self.initial_capital,
        }
    }

    fn selected_value(&self) -> &str {
        match self.selected {
            QuickEditField::RunId => &self.run_id.value,
            QuickEditField::Symbol => &self.symbol.value,
            QuickEditField::Timeframe => &self.timeframe.value,
            QuickEditField::InitialCapital => &self.initial_capital.value,
        }
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
    pub quick_edit: QuickEditState,
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

    pub reports_mode: ReportsMode,
    pub reports_runs: Vec<ReportsRun>,
    pub reports_selected_run: usize,
    pub reports_analyzers: Vec<String>,
    pub reports_selected_analyzer: usize,
    pub reports_analyzer_text: Option<String>,
    pub reports_scroll: usize,

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
            quick_edit: QuickEditState::new(),
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
            reports_mode: ReportsMode::Runs,
            reports_runs: Vec::new(),
            reports_selected_run: 0,
            reports_analyzers: Vec::new(),
            reports_selected_analyzer: 0,
            reports_analyzer_text: None,
            reports_scroll: 0,
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
            ViewId::Reports => self.handle_reports_keys(key),
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
                    3 => {
                        self.refresh_reports_runs();
                        self.reports_mode = ReportsMode::Runs;
                        ViewId::Reports
                    }
                    _ => ViewId::Setup,
                };
                self.dirty = true;
            }
            _ => {}
        }
        Ok(false)
    }

    fn handle_reports_keys(&mut self, key: KeyEvent) -> Result<bool, String> {
        match key.code {
            KeyCode::Esc => {
                match self.reports_mode {
                    ReportsMode::Runs => {
                        self.active_view = ViewId::MainMenu;
                    }
                    ReportsMode::AnalyzerList => {
                        self.reports_mode = ReportsMode::Runs;
                        self.reports_analyzers.clear();
                        self.reports_selected_analyzer = 0;
                        self.reports_analyzer_text = None;
                        self.reports_scroll = 0;
                    }
                    ReportsMode::AnalyzerDetail => {
                        self.reports_mode = ReportsMode::AnalyzerList;
                        self.reports_analyzer_text = None;
                        self.reports_scroll = 0;
                    }
                }
                self.dirty = true;
            }
            KeyCode::Char('g') | KeyCode::F(5) => {
                self.refresh_reports_runs();
                self.info_message = Some("refreshed runs list".to_string());
                self.info_expires_at = Some(Instant::now() + std::time::Duration::from_secs(2));
                self.dirty = true;
            }
            KeyCode::Up => match self.reports_mode {
                ReportsMode::Runs => {
                    self.reports_selected_run = self.reports_selected_run.saturating_sub(1);
                    self.dirty = true;
                }
                ReportsMode::AnalyzerList => {
                    self.reports_selected_analyzer =
                        self.reports_selected_analyzer.saturating_sub(1);
                    self.dirty = true;
                }
                ReportsMode::AnalyzerDetail => {
                    self.reports_scroll = self.reports_scroll.saturating_sub(1);
                    self.dirty = true;
                }
            },
            KeyCode::Down => match self.reports_mode {
                ReportsMode::Runs => {
                    let max = self.reports_runs.len().saturating_sub(1);
                    self.reports_selected_run = (self.reports_selected_run + 1).min(max);
                    self.dirty = true;
                }
                ReportsMode::AnalyzerList => {
                    let max = self.reports_analyzers.len().saturating_sub(1);
                    self.reports_selected_analyzer = (self.reports_selected_analyzer + 1).min(max);
                    self.dirty = true;
                }
                ReportsMode::AnalyzerDetail => {
                    self.reports_scroll = self.reports_scroll.saturating_add(1);
                    self.dirty = true;
                }
            },
            KeyCode::PageUp => {
                if self.reports_mode == ReportsMode::AnalyzerDetail {
                    self.reports_scroll = self.reports_scroll.saturating_add(5);
                } else {
                    self.log_scroll = self.log_scroll.saturating_add(3);
                }
                self.dirty = true;
            }
            KeyCode::PageDown => {
                if self.reports_mode == ReportsMode::AnalyzerDetail {
                    self.reports_scroll = self.reports_scroll.saturating_sub(5);
                } else {
                    self.log_scroll = self.log_scroll.saturating_sub(3);
                }
                self.dirty = true;
            }
            KeyCode::Enter => match self.reports_mode {
                ReportsMode::Runs => {
                    if let Some(run) = self.reports_runs.get(self.reports_selected_run).cloned() {
                        self.refresh_reports_analyzers(&run.run_id);
                        if self.reports_analyzers.is_empty() {
                            self.info_message = Some("no analyzers found for run".to_string());
                            self.info_expires_at =
                                Some(Instant::now() + std::time::Duration::from_secs(2));
                        } else {
                            self.reports_mode = ReportsMode::AnalyzerList;
                        }
                        self.dirty = true;
                    }
                }
                ReportsMode::AnalyzerList => {
                    if let Some(run) = self.reports_runs.get(self.reports_selected_run) {
                        if let Some(analyzer) =
                            self.reports_analyzers.get(self.reports_selected_analyzer)
                        {
                            match self.load_analyzer_text(&run.run_id, analyzer) {
                                Ok(text) => {
                                    self.reports_analyzer_text = Some(text);
                                    self.reports_scroll = 0;
                                    self.reports_mode = ReportsMode::AnalyzerDetail;
                                }
                                Err(err) => {
                                    self.last_error = Some(err);
                                }
                            }
                            self.dirty = true;
                        }
                    }
                }
                ReportsMode::AnalyzerDetail => {}
            },
            _ => {}
        }
        Ok(false)
    }

    fn reports_out_dir(&self) -> PathBuf {
        self.config
            .as_ref()
            .map(|c| PathBuf::from(&c.paths.out_dir))
            .unwrap_or_else(|| self.default_out_dir.clone())
    }

    fn refresh_reports_runs(&mut self) {
        let out_dir = self.reports_out_dir();
        let mut entries: Vec<_> = std::fs::read_dir(&out_dir)
            .ok()
            .into_iter()
            .flat_map(|it| it.filter_map(|e| e.ok()).collect::<Vec<_>>())
            .collect();
        entries.sort_by_key(|e| e.file_name());

        let mut runs: Vec<ReportsRun> = Vec::new();
        for e in entries {
            if !e.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
                continue;
            }
            let run_id = e.file_name().to_string_lossy().to_string();
            let run_dir = out_dir.join(&run_id);
            let summary_path = run_dir.join("summary.json");

            let analyzer_count = std::fs::read_dir(run_dir.join("analyzers"))
                .ok()
                .map(|it| {
                    it.filter_map(|e| e.ok())
                        .filter(|e| {
                            e.path()
                                .extension()
                                .and_then(|s| s.to_str())
                                .is_some_and(|s| s.eq_ignore_ascii_case("json"))
                        })
                        .count()
                })
                .unwrap_or(0);

            let line = if summary_path.exists() {
                match std::fs::read_to_string(&summary_path)
                    .ok()
                    .and_then(|c| serde_json::from_str::<serde_json::Value>(&c).ok())
                {
                    Some(value) => {
                        let summary = value.get("summary").unwrap_or(&value);
                        let net_profit = summary.get("net_profit").and_then(|v| v.as_f64());
                        let sharpe = summary.get("sharpe").and_then(|v| v.as_f64());
                        let max_drawdown = summary.get("max_drawdown").and_then(|v| v.as_f64());
                        format!(
                            "{run_id}: net_profit={:?} sharpe={:?} max_dd={:?} analyzers={}",
                            net_profit, sharpe, max_drawdown, analyzer_count
                        )
                    }
                    None => format!("{run_id} (invalid summary.json) analyzers={analyzer_count}"),
                }
            } else {
                format!("{run_id} (no summary.json) analyzers={analyzer_count}")
            };

            runs.push(ReportsRun { run_id, line });
        }

        self.reports_runs = runs;
        let max = self.reports_runs.len().saturating_sub(1);
        self.reports_selected_run = self.reports_selected_run.min(max);
        self.reports_mode = ReportsMode::Runs;
        self.reports_analyzers.clear();
        self.reports_selected_analyzer = 0;
        self.reports_analyzer_text = None;
        self.reports_scroll = 0;
    }

    fn refresh_reports_analyzers(&mut self, run_id: &str) {
        let out_dir = self.reports_out_dir();
        let dir = out_dir.join(run_id).join("analyzers");
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .ok()
            .into_iter()
            .flat_map(|it| it.filter_map(|e| e.ok()).collect::<Vec<_>>())
            .filter_map(|e| {
                let path = e.path();
                if path
                    .extension()
                    .and_then(|s| s.to_str())
                    .is_some_and(|s| s.eq_ignore_ascii_case("json"))
                {
                    path.file_name()
                        .and_then(|s| s.to_str())
                        .map(|s| s.to_string())
                } else {
                    None
                }
            })
            .collect();
        names.sort();
        self.reports_analyzers = names;
        let max = self.reports_analyzers.len().saturating_sub(1);
        self.reports_selected_analyzer = self.reports_selected_analyzer.min(max);
        self.reports_analyzer_text = None;
        self.reports_scroll = 0;
    }

    fn load_analyzer_text(&self, run_id: &str, analyzer_file: &str) -> Result<String, String> {
        let out_dir = self.reports_out_dir();
        let path = out_dir.join(run_id).join("analyzers").join(analyzer_file);
        let raw = std::fs::read_to_string(&path)
            .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
        let value: serde_json::Value = serde_json::from_str(&raw)
            .map_err(|err| format!("failed to parse {}: {err}", path.display()))?;
        serde_json::to_string_pretty(&value).map_err(|err| format!("failed to format json: {err}"))
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
                    SetupFocus::List => SetupFocus::QuickEdit,
                    SetupFocus::QuickEdit => SetupFocus::Input,
                };
                self.dirty = true;
            }
            KeyCode::Char('i') => {
                self.setup_focus = SetupFocus::Input;
                self.dirty = true;
            }
            KeyCode::Char('e') => {
                self.setup_focus = SetupFocus::QuickEdit;
                self.dirty = true;
            }
            KeyCode::Char('l') => {
                self.setup_focus = SetupFocus::List;
                self.dirty = true;
            }
            KeyCode::Enter => {
                match self.setup_focus {
                    SetupFocus::Input => self.try_load_config(),
                    SetupFocus::QuickEdit => self.apply_quick_edit_selected(),
                    SetupFocus::List => self.try_load_selected_config(),
                }
                self.dirty = true;
            }
            KeyCode::Up => {
                if self.setup_focus == SetupFocus::List {
                    self.selected_config = self.selected_config.saturating_sub(1);
                    self.dirty = true;
                } else if self.setup_focus == SetupFocus::QuickEdit {
                    self.quick_edit.selected = self.quick_edit.selected.prev();
                    self.dirty = true;
                }
            }
            KeyCode::Down => {
                if self.setup_focus == SetupFocus::List {
                    let max = self.available_configs.len().saturating_sub(1);
                    self.selected_config = (self.selected_config + 1).min(max);
                    self.dirty = true;
                } else if self.setup_focus == SetupFocus::QuickEdit {
                    self.quick_edit.selected = self.quick_edit.selected.next();
                    self.dirty = true;
                }
            }
            KeyCode::Backspace => {
                if self.setup_focus == SetupFocus::Input {
                    self.config_input.backspace();
                    self.dirty = true;
                } else if self.setup_focus == SetupFocus::QuickEdit {
                    self.quick_edit.selected_input_mut().backspace();
                    self.dirty = true;
                }
            }
            KeyCode::Delete => {
                if self.setup_focus == SetupFocus::Input {
                    self.config_input.delete();
                    self.dirty = true;
                } else if self.setup_focus == SetupFocus::QuickEdit {
                    self.quick_edit.selected_input_mut().delete();
                    self.dirty = true;
                }
            }
            KeyCode::Left => {
                if self.setup_focus == SetupFocus::Input {
                    self.config_input.move_left();
                    self.dirty = true;
                } else if self.setup_focus == SetupFocus::QuickEdit {
                    self.quick_edit.selected_input_mut().move_left();
                    self.dirty = true;
                }
            }
            KeyCode::Right => {
                if self.setup_focus == SetupFocus::Input {
                    self.config_input.move_right();
                    self.dirty = true;
                } else if self.setup_focus == SetupFocus::QuickEdit {
                    self.quick_edit.selected_input_mut().move_right();
                    self.dirty = true;
                }
            }
            KeyCode::Char(ch) => {
                if !key.modifiers.contains(KeyModifiers::CONTROL)
                    && self.setup_focus == SetupFocus::Input
                {
                    self.config_input.insert_char(ch);
                    self.dirty = true;
                } else if !key.modifiers.contains(KeyModifiers::CONTROL)
                    && self.setup_focus == SetupFocus::QuickEdit
                {
                    self.quick_edit.selected_input_mut().insert_char(ch);
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
            KeyCode::Char('n') => {
                if self.status.running
                    && self.paused
                    && self.status.kind == Some(TaskKind::Backtest)
                {
                    let _ = self.task_runner.step_once();
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
                self.quick_edit.sync_from_config(&cfg);
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

    fn apply_quick_edit_selected(&mut self) {
        let Some(current) = self.config.as_ref().map(|c| (**c).clone()) else {
            self.last_error = Some("load a config first".to_string());
            return;
        };

        let raw = self.quick_edit.selected_value().trim();
        let field = self.quick_edit.selected;

        let mut next = current;
        let result: Result<(), String> = match field {
            QuickEditField::RunId => {
                if raw.is_empty() {
                    Err("run_id cannot be empty".to_string())
                } else {
                    next.run.run_id = raw.to_string();
                    Ok(())
                }
            }
            QuickEditField::Symbol => {
                if raw.is_empty() {
                    Err("symbol cannot be empty".to_string())
                } else {
                    next.run.symbol = raw.to_string();
                    Ok(())
                }
            }
            QuickEditField::Timeframe => {
                if raw.is_empty() {
                    Err("timeframe cannot be empty".to_string())
                } else {
                    next.run.timeframe = raw.to_string();
                    Ok(())
                }
            }
            QuickEditField::InitialCapital => {
                if raw.is_empty() {
                    return self.set_error_and_clear_info("initial_capital cannot be empty");
                }
                let value: f64 = match raw.parse() {
                    Ok(v) => v,
                    Err(_) => {
                        self.set_error_and_clear_info("initial_capital must be a number");
                        return;
                    }
                };
                if !value.is_finite() || value <= 0.0 {
                    Err("initial_capital must be > 0".to_string())
                } else {
                    next.run.initial_capital = value;
                    Ok(())
                }
            }
        };

        if let Err(err) = result {
            self.set_error_and_clear_info(&err);
            return;
        }

        let config_toml = match kairos_application::config::to_toml_pretty(&next) {
            Ok(s) => s,
            Err(err) => {
                self.set_error_and_clear_info(&err);
                return;
            }
        };

        self.config = Some(Arc::new(next));
        self.config_toml = Some(config_toml);
        self.last_error = None;
        self.info_message = Some("quick edit applied".to_string());
        self.info_expires_at = Some(Instant::now() + std::time::Duration::from_secs(2));
    }

    fn set_error_and_clear_info(&mut self, msg: &str) {
        self.last_error = Some(msg.to_string());
        self.info_message = None;
        self.info_expires_at = None;
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
        store_recent_configs_to, App, QuickEditField, SetupFocus, TextInput,
    };
    use crate::logging::LogStore;
    use crate::tasks::TaskRunner;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn minimal_config() -> kairos_application::config::Config {
        kairos_application::config::Config {
            run: kairos_application::config::RunConfig {
                run_id: "x".to_string(),
                symbol: "BTC-USDT".to_string(),
                timeframe: "1min".to_string(),
                initial_capital: 100.0,
            },
            db: kairos_application::config::DbConfig {
                url: None,
                ohlcv_table: "ohlcv_candles".to_string(),
                exchange: "kucoin".to_string(),
                market: "spot".to_string(),
                source_timeframe: None,
                pool_max_size: None,
            },
            paths: kairos_application::config::PathsConfig {
                sentiment_path: None,
                out_dir: "runs/".to_string(),
            },
            costs: kairos_application::config::CostsConfig {
                fee_bps: 0.0,
                slippage_bps: 0.0,
            },
            risk: kairos_application::config::RiskConfig {
                max_position_qty: 1.0,
                max_drawdown_pct: 1.0,
                max_exposure_pct: 1.0,
            },
            orders: None,
            execution: None,
            features: kairos_application::config::FeaturesConfig {
                return_mode: kairos_domain::services::features::ReturnMode::Pct,
                sma_windows: vec![2],
                volatility_windows: None,
                rsi_enabled: false,
                sentiment_lag: "0s".to_string(),
                sentiment_missing: None,
            },
            agent: kairos_application::config::AgentConfig {
                mode: kairos_application::config::AgentMode::Baseline,
                url: "http://127.0.0.1:8000".to_string(),
                timeout_ms: 200,
                retries: 0,
                fallback_action: kairos_domain::value_objects::action_type::ActionType::Hold,
                api_version: "v1".to_string(),
                feature_version: "v1".to_string(),
            },
            strategy: None,
            metrics: None,
            data_quality: None,
            paper: None,
            report: None,
        }
    }

    fn make_app() -> App {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let runner = TaskRunner::new(tx);
        App::new(
            None,
            PathBuf::from("runs"),
            Arc::new(parking_lot::Mutex::new(LogStore::new(10))),
            runner,
        )
    }

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

    #[test]
    fn quick_edit_rejects_invalid_initial_capital() {
        let mut app = make_app();
        let cfg = minimal_config();
        app.config_toml = Some(kairos_application::config::to_toml_pretty(&cfg).unwrap());
        app.config = Some(Arc::new(cfg));
        app.quick_edit
            .sync_from_config(app.config.as_ref().unwrap().as_ref());

        app.setup_focus = SetupFocus::QuickEdit;
        app.quick_edit.selected = QuickEditField::InitialCapital;
        app.quick_edit.initial_capital = TextInput::new("abc".to_string());

        app.apply_quick_edit_selected();

        assert!(app.last_error.is_some());
        assert_eq!(app.config.as_ref().unwrap().run.initial_capital, 100.0);
    }

    #[test]
    fn quick_edit_updates_config_and_toml_snapshot() {
        let mut app = make_app();
        let cfg = minimal_config();
        app.config_toml = Some(kairos_application::config::to_toml_pretty(&cfg).unwrap());
        app.config = Some(Arc::new(cfg));
        app.quick_edit
            .sync_from_config(app.config.as_ref().unwrap().as_ref());

        app.setup_focus = SetupFocus::QuickEdit;
        app.quick_edit.selected = QuickEditField::RunId;
        app.quick_edit.run_id = TextInput::new("run_123".to_string());
        app.apply_quick_edit_selected();

        assert!(app.last_error.is_none());
        assert_eq!(app.config.as_ref().unwrap().run.run_id, "run_123");
        let toml = app.config_toml.as_deref().unwrap_or("");
        assert!(toml.contains("run_id = \"run_123\""));
    }
}
