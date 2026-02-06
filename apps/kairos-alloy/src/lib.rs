mod app;
pub mod bootstrap;
pub mod headless;
pub mod logging;
mod tasks;
mod ui;

use crate::app::{App, ViewId};
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::{execute, ExecutableCommand};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone)]
pub struct TuiOpts {
    pub initial_config_path: Option<PathBuf>,
    pub log_store: Arc<parking_lot::Mutex<logging::LogStore>>,
    pub default_out_dir: PathBuf,
}

pub fn run(opts: TuiOpts) -> Result<(), String> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_time()
        .build()
        .map_err(|err| format!("failed to init tokio runtime: {err}"))?;
    runtime.block_on(run_async(opts))
}

async fn run_async(opts: TuiOpts) -> Result<(), String> {
    enable_raw_mode().map_err(|err| format!("failed to enable raw mode: {err}"))?;

    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)
        .map_err(|err| format!("failed to enter alternate screen: {err}"))?;
    stdout
        .execute(crossterm::terminal::Clear(
            crossterm::terminal::ClearType::All,
        ))
        .map_err(|err| format!("failed to clear screen: {err}"))?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal =
        Terminal::new(backend).map_err(|err| format!("failed to init terminal: {err}"))?;
    terminal
        .hide_cursor()
        .map_err(|err| format!("failed to hide cursor: {err}"))?;

    let result = run_loop(&mut terminal, opts).await;

    let mut stdout = io::stdout();
    let _ = execute!(stdout, LeaveAlternateScreen, DisableMouseCapture);
    let _ = disable_raw_mode();
    let _ = terminal.show_cursor();

    result
}

async fn run_loop<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    opts: TuiOpts,
) -> Result<(), String> {
    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();
    let task_runner = tasks::TaskRunner::new(event_tx.clone());
    let mut app = App::new(
        opts.initial_config_path,
        opts.default_out_dir,
        opts.log_store,
        task_runner,
    );

    if app.config_path.is_some() {
        app.try_load_config();
        app.active_view = ViewId::Setup;
    }

    app.spawn_input_reader(event_tx);

    let mut tick = tokio::time::interval(Duration::from_millis(33));

    loop {
        if app.dirty {
            terminal
                .draw(|frame| ui::draw(frame, &mut app))
                .map_err(|err| format!("terminal draw failed: {err}"))?;
            app.dirty = false;
        }

        tokio::select! {
            _ = tick.tick() => {
                app.on_tick();
            }
            maybe_event = event_rx.recv() => {
                let Some(event) = maybe_event else { return Ok(()); };
                if app.on_event(event)? { return Ok(()); }
            }
        }
    }
}
