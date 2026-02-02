use kairos_domain::repositories::agent::AgentClient as AgentPort;
use kairos_domain::repositories::market_data::MarketDataRepository;
use kairos_domain::repositories::sentiment::SentimentRepository;
use kairos_infrastructure::agents::AgentClient as InfraAgentClient;
use kairos_infrastructure::artifacts::FilesystemArtifactWriter;
use kairos_infrastructure::persistence::postgres_ohlcv::PostgresMarketDataRepository;
use kairos_infrastructure::sentiment::FilesystemSentimentRepository;
use parking_lot::{Condvar, Mutex};
use std::env;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

const STREAM_EVERY_N_BARS: u64 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskKind {
    Validate { strict: bool },
    Backtest,
    Paper,
}

#[derive(Debug, Clone)]
pub struct TradeSample {
    pub bar_index: u64,
    pub timestamp: i64,
    pub side: kairos_domain::value_objects::side::Side,
    pub quantity: f64,
    pub price: f64,
}

#[derive(Debug, Clone)]
pub struct BarProgressSample {
    pub x: f64,
    pub price: f64,
    pub equity: f64,
    pub trades_in_bar: Vec<TradeSample>,
}

pub enum TaskEvent {
    Input(crossterm::event::Event),
    Progress(BarProgressSample),
    TaskFinished(Result<String, String>),
}

#[derive(Clone)]
pub struct TaskRunner {
    inner: Arc<TaskRunnerInner>,
}

struct TaskRunnerInner {
    tx: tokio::sync::mpsc::UnboundedSender<TaskEvent>,
    control: Mutex<Option<TaskControl>>,
}

#[derive(Clone)]
struct TaskControl {
    cancel: Arc<AtomicBool>,
    paused: Arc<(Mutex<bool>, Condvar)>,
}

impl TaskControl {
    fn new() -> Self {
        Self {
            cancel: Arc::new(AtomicBool::new(false)),
            paused: Arc::new((Mutex::new(false), Condvar::new())),
        }
    }

    fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
        let (_, cvar) = &*self.paused;
        cvar.notify_all();
    }

    fn toggle_pause(&self) -> bool {
        let (lock, cvar) = &*self.paused;
        let mut paused = lock.lock();
        *paused = !*paused;
        if !*paused {
            cvar.notify_all();
        }
        *paused
    }
}

impl kairos_domain::services::engine::backtest::RunControl for TaskControl {
    fn should_cancel(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    fn wait_if_paused(&self) -> bool {
        let (lock, cvar) = &*self.paused;
        let mut paused = lock.lock();
        while *paused {
            if self.should_cancel() {
                return false;
            }
            cvar.wait(&mut paused);
        }
        !self.should_cancel()
    }
}

impl TaskRunner {
    pub fn new(tx: tokio::sync::mpsc::UnboundedSender<TaskEvent>) -> Self {
        Self {
            inner: Arc::new(TaskRunnerInner {
                tx,
                control: Mutex::new(None),
            }),
        }
    }

    pub fn start(
        &self,
        kind: TaskKind,
        config: Arc<kairos_application::config::Config>,
        config_toml: String,
    ) {
        let inner = self.inner.clone();
        let tx = inner.tx.clone();
        tokio::task::spawn_blocking(move || {
            let control = match kind {
                TaskKind::Backtest | TaskKind::Paper => Some(TaskControl::new()),
                _ => None,
            };
            {
                let mut slot = inner.control.lock();
                *slot = control.clone();
            }

            let result = run_task(
                kind,
                config.as_ref(),
                &config_toml,
                &tx,
                control
                    .as_ref()
                    .map(|c| c as &dyn kairos_domain::services::engine::backtest::RunControl),
            );
            {
                let mut slot = inner.control.lock();
                *slot = None;
            }
            let _ = tx.send(TaskEvent::TaskFinished(result));
        });
    }

    pub fn cancel_current(&self) {
        let control = { self.inner.control.lock().clone() };
        if let Some(control) = control {
            control.cancel();
        }
    }

    pub fn toggle_pause(&self) -> bool {
        let control = { self.inner.control.lock().clone() };
        control.map(|c| c.toggle_pause()).unwrap_or(false)
    }
}

fn run_task(
    kind: TaskKind,
    config: &kairos_application::config::Config,
    config_toml: &str,
    tx: &tokio::sync::mpsc::UnboundedSender<TaskEvent>,
    control: Option<&dyn kairos_domain::services::engine::backtest::RunControl>,
) -> Result<String, String> {
    match kind {
        TaskKind::Validate { strict } => run_validate(config, strict),
        TaskKind::Backtest => run_backtest(config, config_toml, tx, control),
        TaskKind::Paper => run_paper(config, config_toml, tx, control),
    }
}

fn resolve_db_url(config: &kairos_application::config::Config) -> Result<String, String> {
    match config.db.url.as_deref() {
        Some(url) if !url.trim().is_empty() => Ok(url.to_string()),
        _ => env::var("KAIROS_DB_URL")
            .map_err(|_| "missing db.url in config and env KAIROS_DB_URL is not set".to_string()),
    }
}

fn build_market_data_repo(
    config: &kairos_application::config::Config,
) -> Result<Box<dyn MarketDataRepository>, String> {
    let db_url = resolve_db_url(config)?;
    let pool_max_size = config.db.pool_max_size.unwrap_or(8);
    Ok(Box::new(PostgresMarketDataRepository::new(
        db_url,
        config.db.ohlcv_table.to_string(),
        pool_max_size,
    )?))
}

fn build_sentiment_repo() -> Box<dyn SentimentRepository> {
    Box::new(FilesystemSentimentRepository)
}

fn build_remote_agent(
    config: &kairos_application::config::Config,
) -> Result<Option<Box<dyn AgentPort>>, String> {
    match config.agent.mode {
        kairos_application::config::AgentMode::Remote => {
            let agent = InfraAgentClient::new(
                config.agent.url.clone(),
                config.agent.timeout_ms,
                config.agent.api_version.clone(),
                config.agent.feature_version.clone(),
                config.agent.retries,
                config.agent.fallback_action,
            )
            .map_err(|err| {
                format!(
                    "failed to init remote agent client (url={}): {err}",
                    config.agent.url
                )
            })?;
            Ok(Some(Box::new(agent)))
        }
        _ => Ok(None),
    }
}

fn run_validate(
    config: &kairos_application::config::Config,
    strict: bool,
) -> Result<String, String> {
    let market_data = build_market_data_repo(config)?;
    let sentiment_repo = build_sentiment_repo();

    let report = kairos_application::validation::validate(
        config,
        strict,
        market_data.as_ref(),
        sentiment_repo.as_ref(),
    )?;
    serde_json::to_string_pretty(&report)
        .map_err(|err| format!("failed to serialize validate report: {err}"))
}

fn run_backtest(
    config: &kairos_application::config::Config,
    config_toml: &str,
    tx: &tokio::sync::mpsc::UnboundedSender<TaskEvent>,
    control: Option<&dyn kairos_domain::services::engine::backtest::RunControl>,
) -> Result<String, String> {
    use kairos_domain::services::engine::backtest::BarProgress;

    let market_data = build_market_data_repo(config)?;
    let sentiment_repo = build_sentiment_repo();
    let artifacts = FilesystemArtifactWriter::new();
    let remote_agent = build_remote_agent(config)?;

    let mut last: Option<(f64, f64, f64)> = None;
    let mut last_sent_x: Option<f64> = None;
    let mut progress = |p: BarProgress| {
        let bar_index = p.bar_index;
        let x = bar_index as f64;
        last = Some((x, p.close, p.equity));

        let has_trades = !p.trades_in_bar.is_empty();
        if bar_index.is_multiple_of(STREAM_EVERY_N_BARS) || has_trades {
            let trades_in_bar = if has_trades {
                p.trades_in_bar
                    .into_iter()
                    .map(|t| TradeSample {
                        bar_index,
                        timestamp: t.timestamp,
                        side: t.side,
                        quantity: t.quantity,
                        price: t.price,
                    })
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            };
            let sample = BarProgressSample {
                x,
                price: p.close,
                equity: p.equity,
                trades_in_bar,
            };
            let _ = tx.send(TaskEvent::Progress(sample));
            last_sent_x = Some(x);
        }
    };

    let run_dir = if let Some(control) = control {
        kairos_application::backtesting::run_backtest_streaming_control(
            config,
            config_toml,
            None,
            market_data.as_ref(),
            sentiment_repo.as_ref(),
            &artifacts,
            remote_agent,
            control,
            &mut progress,
        )?
    } else {
        kairos_application::backtesting::run_backtest_streaming(
            config,
            config_toml,
            None,
            market_data.as_ref(),
            sentiment_repo.as_ref(),
            &artifacts,
            remote_agent,
            &mut progress,
        )?
    };
    if let Some((x, price, equity)) = last {
        if last_sent_x != Some(x) {
            let _ = tx.send(TaskEvent::Progress(BarProgressSample {
                x,
                price,
                equity,
                trades_in_bar: Vec::new(),
            }));
        }
    }
    Ok(format!("backtest complete: {}", run_dir.display()))
}

fn run_paper(
    config: &kairos_application::config::Config,
    config_toml: &str,
    tx: &tokio::sync::mpsc::UnboundedSender<TaskEvent>,
    control: Option<&dyn kairos_domain::services::engine::backtest::RunControl>,
) -> Result<String, String> {
    use kairos_domain::services::engine::backtest::BarProgress;

    let market_data = build_market_data_repo(config)?;
    let sentiment_repo = build_sentiment_repo();
    let artifacts = FilesystemArtifactWriter::new();
    let remote_agent = build_remote_agent(config)?;

    let mut last: Option<(f64, f64, f64)> = None;
    let mut last_sent_x: Option<f64> = None;
    let mut progress = |p: BarProgress| {
        let bar_index = p.bar_index;
        let x = bar_index as f64;
        last = Some((x, p.close, p.equity));

        let has_trades = !p.trades_in_bar.is_empty();
        if bar_index.is_multiple_of(STREAM_EVERY_N_BARS) || has_trades {
            let trades_in_bar = if has_trades {
                p.trades_in_bar
                    .into_iter()
                    .map(|t| TradeSample {
                        bar_index,
                        timestamp: t.timestamp,
                        side: t.side,
                        quantity: t.quantity,
                        price: t.price,
                    })
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            };
            let sample = BarProgressSample {
                x,
                price: p.close,
                equity: p.equity,
                trades_in_bar,
            };
            let _ = tx.send(TaskEvent::Progress(sample));
            last_sent_x = Some(x);
        }
    };

    let run_dir = if let Some(control) = control {
        kairos_application::paper_trading::run_paper_streaming_control(
            config,
            config_toml,
            None,
            market_data.as_ref(),
            sentiment_repo.as_ref(),
            &artifacts,
            remote_agent,
            control,
            &mut progress,
        )?
    } else {
        kairos_application::paper_trading::run_paper_streaming(
            config,
            config_toml,
            None,
            market_data.as_ref(),
            sentiment_repo.as_ref(),
            &artifacts,
            remote_agent,
            &mut progress,
        )?
    };
    if let Some((x, price, equity)) = last {
        if last_sent_x != Some(x) {
            let _ = tx.send(TaskEvent::Progress(BarProgressSample {
                x,
                price,
                equity,
                trades_in_bar: Vec::new(),
            }));
        }
    }
    Ok(format!("paper run complete: {}", run_dir.display()))
}
