use crate::app::{App, BacktestTab, QuickEditField, ReportsMode, SetupFocus, ViewId};
use kairos_domain::value_objects::side::Side;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Axis, Block, Borders, Chart, Dataset, GraphType, List, ListItem, Paragraph, Tabs, Wrap,
};
use ratatui::Frame;
use std::path::PathBuf;

pub fn draw(frame: &mut Frame, app: &mut App) {
    let size = frame.area();
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Length(1),
                Constraint::Min(3),
                Constraint::Length(8),
            ]
            .as_ref(),
        )
        .split(size);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(24), Constraint::Min(10)].as_ref())
        .split(outer[1]);

    draw_top_banner(frame, outer[0], app);
    draw_sidebar(frame, body[0], app);
    draw_main(frame, body[1], app);
    draw_bottom(frame, outer[2], app);
}

fn draw_top_banner(frame: &mut Frame, area: Rect, app: &App) {
    let (text, style) = if app.cancel_requested {
        (
            "CANCELLED",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )
    } else if app.paused && app.pause_blink {
        (
            "PAUSED",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        ("", Style::default())
    };

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(text, style))).alignment(Alignment::Center),
        area,
    );
}

fn draw_sidebar(frame: &mut Frame, area: Rect, app: &App) {
    let items = ["Setup", "Backtest", "Monitor", "Reports", "Quit"];
    let list_items: Vec<ListItem> = items
        .iter()
        .enumerate()
        .map(|(idx, label)| {
            let mut style = Style::default();
            if app.active_view == ViewId::MainMenu && idx == app.menu_index {
                style = style.fg(Color::Yellow).add_modifier(Modifier::BOLD);
            }
            ListItem::new(Line::from(Span::styled(*label, style)))
        })
        .collect();

    let block = Block::default().title("Menu").borders(Borders::ALL);
    let list = List::new(list_items).block(block);
    frame.render_widget(list, area);
}

fn draw_main(frame: &mut Frame, area: Rect, app: &mut App) {
    match app.active_view {
        ViewId::MainMenu => draw_main_menu(frame, area),
        ViewId::Setup => draw_setup(frame, area, app),
        ViewId::Backtest => draw_backtest(frame, area, app),
        ViewId::Monitor => draw_monitor(frame, area, app),
        ViewId::Reports => draw_reports(frame, area, app),
    }
}

fn draw_main_menu(frame: &mut Frame, area: Rect) {
    let version = env!("CARGO_PKG_VERSION");
    let git_sha = option_env!("KAIROS_GIT_SHA").unwrap_or("unknown");
    let target = option_env!("KAIROS_TARGET").unwrap_or("unknown");
    let lines = vec![
        Line::from(format!(
            "Kairos Alloy (TUI) v{} ({}, {})",
            version, git_sha, target
        )),
        Line::from(""),
        Line::from("Use ↑/↓ + Enter to navigate."),
        Line::from("Esc returns to menu. Ctrl-C or q quits."),
    ];
    frame.render_widget(
        Paragraph::new(lines).block(Block::default().title("Main").borders(Borders::ALL)),
        area,
    );
}

fn draw_setup(frame: &mut Frame, area: Rect, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Length(10),
                Constraint::Length(9),
                Constraint::Min(6),
                Constraint::Length(6),
            ]
            .as_ref(),
        )
        .split(area);

    let mut header = vec![
        Line::from("Config path (type + Enter to load):"),
        Line::from(app.config_input.value.clone()),
    ];
    if let Some(err) = &app.last_error {
        header.push(Line::from(Span::styled(
            format!("error: {err}"),
            Style::default().fg(Color::Red),
        )));
    }
    if let Some(info) = &app.info_message {
        header.push(Line::from(Span::styled(
            format!("info: {info}"),
            Style::default().fg(Color::Green),
        )));
    }
    if let Some(cfg) = &app.config {
        let cfg = cfg.as_ref();
        header.push(Line::from(""));
        header.push(Line::from(format!(
            "loaded: run_id={} symbol={} timeframe={}",
            cfg.run.run_id, cfg.run.symbol, cfg.run.timeframe
        )));
    }
    if let Some(path) = app.available_configs.get(app.selected_config) {
        header.push(Line::from(""));
        header.push(Line::from(format!("selected: {}", path.display())));
    }

    frame.render_widget(
        Paragraph::new(header)
            .block(
                Block::default()
                    .title(match app.setup_focus {
                        SetupFocus::Input => "Setup (input)",
                        SetupFocus::QuickEdit => "Setup (edit)",
                        SetupFocus::List => "Setup",
                    })
                    .borders(Borders::ALL),
            )
            .wrap(Wrap { trim: false }),
        chunks[0],
    );

    let quick_edit_lines = if app.config.is_none() {
        vec![
            Line::from("Quick Edit"),
            Line::from(""),
            Line::from("Load a config first to enable quick edit."),
        ]
    } else {
        let mut lines: Vec<Line> = Vec::new();
        lines.push(Line::from("Fields (Enter applies selected):"));
        lines.push(Line::from(""));

        let masked_key = {
            let raw = app.quick_edit.llm_api_key.value.as_str();
            if raw.trim().is_empty() {
                "<empty>".to_string()
            } else {
                let t = raw.trim();
                let last4 = t.chars().rev().take(4).collect::<Vec<_>>();
                let last4 = last4.into_iter().rev().collect::<String>();
                format!("************{last4} (len={})", t.chars().count())
            }
        };

        let llm_provider_val = app.quick_edit.llm_provider.value.as_str();
        let llm_model_val = if app.quick_edit.llm_model.value.trim().is_empty() {
            "<default>".to_string()
        } else {
            app.quick_edit.llm_model.value.clone()
        };

        let items: Vec<(QuickEditField, &str, String)> = vec![
            (
                QuickEditField::RunId,
                "run_id",
                app.quick_edit.run_id.value.clone(),
            ),
            (
                QuickEditField::Symbol,
                "symbol",
                app.quick_edit.symbol.value.clone(),
            ),
            (
                QuickEditField::Timeframe,
                "timeframe",
                app.quick_edit.timeframe.value.clone(),
            ),
            (
                QuickEditField::InitialCapital,
                "initial_capital",
                app.quick_edit.initial_capital.value.clone(),
            ),
            (
                QuickEditField::LlmProvider,
                "llm_provider (runtime)",
                llm_provider_val.to_string(),
            ),
            (
                QuickEditField::LlmModel,
                "llm_model (runtime)",
                llm_model_val,
            ),
            (
                QuickEditField::LlmApiKey,
                "llm_api_key (runtime)",
                masked_key,
            ),
            (
                QuickEditField::LlmManagedAgent,
                "llm_managed_agent (runtime)",
                app.quick_edit.llm_managed_agent.value.clone(),
            ),
        ];

        for (field, label, value) in items {
            let marker = if field == app.quick_edit.selected {
                ">"
            } else {
                " "
            };
            let mut style = Style::default();
            if field == app.quick_edit.selected {
                style = style.fg(Color::Yellow).add_modifier(Modifier::BOLD);
                if app.setup_focus != SetupFocus::QuickEdit {
                    style = style.fg(Color::DarkGray).add_modifier(Modifier::BOLD);
                }
            }
            lines.push(Line::from(Span::styled(
                format!("{marker} {label}: {value}"),
                style,
            )));
        }

        lines
    };
    frame.render_widget(
        Paragraph::new(quick_edit_lines)
            .block(Block::default().title("Quick Edit").borders(Borders::ALL))
            .wrap(Wrap { trim: false }),
        chunks[1],
    );

    let mut items: Vec<ListItem> = Vec::new();
    if app.available_configs.is_empty() {
        items.push(ListItem::new(Line::from("no configs found")));
    } else {
        for (idx, path) in app.available_configs.iter().enumerate() {
            let is_recent = idx < app.recent_config_count;
            let label = path
                .file_name()
                .and_then(|s| s.to_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| path.display().to_string());
            let prefix = if is_recent { "recent" } else { "config" };
            let marker = if is_recent { "*" } else { " " };

            let mut style = Style::default();
            if idx == app.selected_config {
                style = style.fg(Color::Yellow).add_modifier(Modifier::BOLD);
                if app.setup_focus != SetupFocus::List {
                    style = style.fg(Color::DarkGray).add_modifier(Modifier::BOLD);
                }
            }
            items.push(ListItem::new(Line::from(Span::styled(
                format!("{marker} {prefix}: {label}"),
                style,
            ))));
        }
    }
    let list_title = match app.setup_focus {
        SetupFocus::List => "Configs (focused)",
        SetupFocus::Input | SetupFocus::QuickEdit => "Configs",
    };
    frame.render_widget(
        List::new(items).block(Block::default().title(list_title).borders(Borders::ALL)),
        chunks[2],
    );

    let help = vec![
        Line::from("Keys:"),
        Line::from("  Tab: switch input/list/edit"),
        Line::from("  Enter: load/apply (focused)"),
        Line::from("  ↑/↓: select (list/edit)"),
        Line::from("  i/l/e: focus input/list/edit"),
        Line::from("  g/F5: refresh list"),
        Line::from("  Note: llm_* fields are runtime-only (not saved to config_snapshot)"),
        Line::from("  Note: llm_managed_agent=on spawns python agent (dev checkout only)"),
        Line::from("  Esc: back to menu"),
    ];
    frame.render_widget(
        Paragraph::new(help).block(Block::default().title("Help").borders(Borders::ALL)),
        chunks[3],
    );
}

fn draw_backtest(frame: &mut Frame, area: Rect, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(3)].as_ref())
        .split(area);

    let tab_titles: Vec<Line> = vec!["Validate", "Backtest", "Paper"]
        .into_iter()
        .map(Line::from)
        .collect();
    let tab_index = match app.backtest_tab {
        BacktestTab::Validate => 0,
        BacktestTab::Backtest => 1,
        BacktestTab::Paper => 2,
    };
    let tabs = Tabs::new(tab_titles)
        .select(tab_index)
        .block(Block::default().title("Mode").borders(Borders::ALL))
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
    frame.render_widget(tabs, chunks[0]);

    let mut lines: Vec<Line> = Vec::new();
    if app.status.running {
        let kind = app.status.kind.map(task_kind_label).unwrap_or("task");
        let state = if app.cancel_requested {
            "CANCELLED"
        } else if app.paused {
            "PAUSED"
        } else {
            "running"
        };
        lines.push(Line::from(Span::styled(
            format!("{kind} {state} {}", app.spinner_char()),
            Style::default().fg(Color::Yellow),
        )));
    } else {
        lines.push(Line::from("idle"));
    }

    if app.backtest_tab == BacktestTab::Validate {
        lines.push(Line::from(format!(
            "strict: {} (toggle: s)",
            if app.validate_strict { "on" } else { "off" }
        )));
    }
    if app.backtest_tab == BacktestTab::Paper {
        lines.push(Line::from(format!(
            "paper realtime: {} (toggle: t)",
            if app.paper_realtime { "on" } else { "off" }
        )));
    }
    lines.push(Line::from(format!(
        "require validate: {} (toggle: v)",
        if app.require_validate_before_run {
            "on"
        } else {
            "off"
        }
    )));

    if let (Some(kind), Some(status)) = (app.status.kind, app.stream_status.as_ref()) {
        if kind == crate::tasks::TaskKind::PaperRealtime {
            let conn = if status.connected {
                "connected"
            } else {
                "reconnecting"
            };
            lines.push(Line::from(format!(
                "ws: {conn} | reconnects: {} | last_ts: {:?}",
                status.reconnects, status.last_event_timestamp
            )));
            lines.push(Line::from(format!(
                "ws quality: out_of_order={} invalid={}",
                status.out_of_order_events, status.invalid_events
            )));
            if let Some(err) = &status.last_error {
                lines.push(Line::from(Span::styled(
                    format!("ws last_error: {err}"),
                    Style::default().fg(Color::Red),
                )));
            }
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(
        "keys: r run | p pause/resume | n step | x stop | v gate | t paper mode | Esc menu | ←/→ switch tab",
    ));

    if let Some(last) = &app.status.last_result {
        lines.push(Line::from(""));
        match last {
            Ok(msg) => {
                lines.push(Line::from(Span::styled(
                    "last result: OK",
                    Style::default().fg(Color::Green),
                )));
                lines.extend(msg.lines().take(12).map(Line::from));
            }
            Err(err) => {
                lines.push(Line::from(Span::styled(
                    "last result: ERR",
                    Style::default().fg(Color::Red),
                )));
                lines.extend(err.lines().take(12).map(Line::from));
            }
        }
    }

    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().title("Run").borders(Borders::ALL))
            .wrap(Wrap { trim: false }),
        chunks[1],
    );
}

fn draw_monitor(frame: &mut Frame, area: Rect, app: &mut App) {
    if app.price_series.is_empty() || app.equity_series.is_empty() {
        let lines = vec![
            Line::from("Monitor"),
            Line::from(""),
            Line::from("Waiting for progress stream..."),
            Line::from("Run Backtest/Paper to see charts update in real time."),
            Line::from(
                "Keys: p pause/resume, n step (paused, backtest), x stop, ↑/↓ scroll trades, PgUp/PgDn scroll logs.",
            ),
        ];
        frame.render_widget(
            Paragraph::new(lines)
                .block(Block::default().title("Monitor").borders(Borders::ALL))
                .wrap(Wrap { trim: false }),
            area,
        );
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(70), Constraint::Percentage(30)].as_ref())
        .split(area);
    let charts = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
        .split(chunks[0]);

    let price_data: Vec<(f64, f64)> = app.price_series.iter().copied().collect();
    let equity_data: Vec<(f64, f64)> = app.equity_series.iter().copied().collect();

    let (x_min, x_max) = x_bounds(&price_data);

    let (p_min, p_max) = y_bounds(&price_data);
    let (e_min, e_max) = y_bounds(&equity_data);

    let price = Chart::new(vec![Dataset::default()
        .name("price")
        .graph_type(GraphType::Line)
        .style(Style::default().fg(Color::Cyan))
        .data(&price_data)])
    .block(
        Block::default()
            .title("Price (close)")
            .borders(Borders::ALL),
    )
    .x_axis(
        Axis::default()
            .bounds([x_min, x_max])
            .labels(axis_labels(x_min, x_max)),
    )
    .y_axis(
        Axis::default()
            .bounds([p_min, p_max])
            .labels(axis_labels(p_min, p_max)),
    );

    let equity = Chart::new(vec![Dataset::default()
        .name("equity")
        .graph_type(GraphType::Line)
        .style(Style::default().fg(Color::Green))
        .data(&equity_data)])
    .block(Block::default().title("Equity").borders(Borders::ALL))
    .x_axis(
        Axis::default()
            .bounds([x_min, x_max])
            .labels(axis_labels(x_min, x_max)),
    )
    .y_axis(
        Axis::default()
            .bounds([e_min, e_max])
            .labels(axis_labels(e_min, e_max)),
    );

    frame.render_widget(price, charts[0]);
    frame.render_widget(equity, charts[1]);

    let max_lines = chunks[1].height.saturating_sub(2) as usize;
    let mut lines: Vec<Line> = Vec::new();
    if app.trades.is_empty() {
        lines.push(Line::from("no trades yet"));
    } else {
        for trade in app
            .trades
            .iter()
            .rev()
            .skip(app.trade_scroll)
            .take(max_lines)
        {
            let side_style = match trade.side {
                Side::Buy => Style::default().fg(Color::Green),
                Side::Sell => Style::default().fg(Color::Red),
            };
            lines.push(Line::from(vec![
                Span::raw(format!("[#{}] ", trade.bar_index)),
                Span::styled(
                    format!("{:?}", trade.side),
                    side_style.add_modifier(Modifier::BOLD),
                ),
                Span::raw(format!(
                    " qty={:.4} @ {:.2} ts={}",
                    trade.quantity, trade.price, trade.timestamp
                )),
            ]));
        }
    }

    let mut title = "Trades".to_string();
    if app.cancel_requested {
        title.push_str(" (CANCELLED)");
    } else if app.paused {
        title.push_str(" (PAUSED)");
    }
    if let (Some(kind), Some(status)) = (app.status.kind, app.stream_status.as_ref()) {
        if kind == crate::tasks::TaskKind::PaperRealtime {
            title.push_str(if status.connected {
                " (WS: connected)"
            } else {
                " (WS: reconnecting)"
            });
        }
    }
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().title(title).borders(Borders::ALL))
            .wrap(Wrap { trim: false }),
        chunks[1],
    );
}

fn x_bounds(points: &[(f64, f64)]) -> (f64, f64) {
    let x_min = points.first().map(|p| p.0).unwrap_or(0.0);
    let mut x_max = points.last().map(|p| p.0).unwrap_or(x_min + 1.0);
    if x_max <= x_min {
        x_max = x_min + 1.0;
    }
    (x_min, x_max)
}

fn y_bounds(points: &[(f64, f64)]) -> (f64, f64) {
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for (_, y) in points {
        min = min.min(*y);
        max = max.max(*y);
    }
    if !min.is_finite() || !max.is_finite() {
        return (0.0, 1.0);
    }
    if max <= min {
        return (min - 1.0, max + 1.0);
    }
    let pad = (max - min) * 0.05;
    (min - pad, max + pad)
}

fn axis_labels(min: f64, max: f64) -> Vec<Line<'static>> {
    let mid = (min + max) / 2.0;
    vec![
        Line::from(format!("{min:.2}")),
        Line::from(format!("{mid:.2}")),
        Line::from(format!("{max:.2}")),
    ]
}

fn draw_reports(frame: &mut Frame, area: Rect, app: &mut App) {
    let out_dir = app
        .config
        .as_ref()
        .map(|c| PathBuf::from(&c.paths.out_dir))
        .unwrap_or_else(|| app.default_out_dir.clone());

    match app.reports_mode {
        ReportsMode::AnalyzerDetail => {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(5), Constraint::Min(3)].as_ref())
                .split(area);

            let run_id = app
                .reports_runs
                .get(app.reports_selected_run)
                .map(|r| r.run_id.as_str())
                .unwrap_or("unknown");
            let analyzer = app
                .reports_analyzers
                .get(app.reports_selected_analyzer)
                .map(|s| s.as_str())
                .unwrap_or("unknown");

            let mut header = vec![Line::from(format!("Runs directory: {}", out_dir.display()))];
            header.push(Line::from(format!("run: {run_id} | analyzer: {analyzer}")));
            header.push(Line::from(
                "keys: ↑/↓ scroll | PgUp/PgDn scroll | g refresh | Esc back",
            ));
            if let Some(err) = &app.last_error {
                header.push(Line::from(Span::styled(
                    format!("error: {err}"),
                    Style::default().fg(Color::Red),
                )));
            }
            if let Some(info) = &app.info_message {
                header.push(Line::from(Span::styled(
                    format!("info: {info}"),
                    Style::default().fg(Color::Green),
                )));
            }

            frame.render_widget(
                Paragraph::new(header)
                    .block(Block::default().title("Analyzer").borders(Borders::ALL))
                    .wrap(Wrap { trim: false }),
                chunks[0],
            );

            let text = app
                .reports_analyzer_text
                .as_deref()
                .unwrap_or("(no analyzer loaded)");
            let scroll = (app.reports_scroll.min(u16::MAX as usize) as u16, 0);
            frame.render_widget(
                Paragraph::new(text.to_string())
                    .block(Block::default().title("JSON").borders(Borders::ALL))
                    .wrap(Wrap { trim: false })
                    .scroll(scroll),
                chunks[1],
            );
        }
        ReportsMode::Runs | ReportsMode::AnalyzerList => {
            let mut lines = vec![Line::from(format!("Runs directory: {}", out_dir.display()))];
            if let Some(err) = &app.last_error {
                lines.push(Line::from(Span::styled(
                    format!("error: {err}"),
                    Style::default().fg(Color::Red),
                )));
            }
            if let Some(info) = &app.info_message {
                lines.push(Line::from(Span::styled(
                    format!("info: {info}"),
                    Style::default().fg(Color::Green),
                )));
            }
            lines.push(Line::from(""));

            let title = match app.reports_mode {
                ReportsMode::Runs => {
                    lines.push(Line::from(
                        "keys: ↑/↓ select | Enter analyzers | g refresh | Esc menu",
                    ));
                    lines.push(Line::from(""));
                    if app.reports_runs.is_empty() {
                        lines.push(Line::from("no runs found (press g to refresh)"));
                    } else {
                        for (idx, run) in app.reports_runs.iter().enumerate() {
                            let prefix = if idx == app.reports_selected_run {
                                "> "
                            } else {
                                "  "
                            };
                            lines.push(Line::from(format!("{prefix}{}", run.line)));
                        }
                    }
                    "Reports (runs)"
                }
                ReportsMode::AnalyzerList => {
                    let run_id = app
                        .reports_runs
                        .get(app.reports_selected_run)
                        .map(|r| r.run_id.as_str())
                        .unwrap_or("unknown");
                    lines.push(Line::from(format!(
                        "run: {run_id} | keys: ↑/↓ select | Enter open | Esc back"
                    )));
                    lines.push(Line::from(""));
                    if app.reports_analyzers.is_empty() {
                        lines.push(Line::from("no analyzers found"));
                    } else {
                        for (idx, name) in app.reports_analyzers.iter().enumerate() {
                            let prefix = if idx == app.reports_selected_analyzer {
                                "> "
                            } else {
                                "  "
                            };
                            lines.push(Line::from(format!("{prefix}{name}")));
                        }
                    }
                    "Reports (analyzers)"
                }
                ReportsMode::AnalyzerDetail => "Reports",
            };

            frame.render_widget(
                Paragraph::new(lines)
                    .block(Block::default().title(title).borders(Borders::ALL))
                    .wrap(Wrap { trim: false }),
                area,
            );
        }
    }
}

fn draw_bottom(frame: &mut Frame, area: Rect, app: &App) {
    let logs = app.logs.lock().snapshot();
    let max_lines = area.height.saturating_sub(2) as usize;

    let start_from_end = app.log_scroll.min(logs.len());
    let mut visible: Vec<String> = logs
        .into_iter()
        .rev()
        .skip(start_from_end)
        .take(max_lines)
        .collect();
    visible.reverse();

    let text: Vec<Line> = visible.into_iter().map(Line::from).collect();
    frame.render_widget(
        Paragraph::new(text)
            .block(Block::default().title("Logs").borders(Borders::ALL))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn task_kind_label(kind: crate::tasks::TaskKind) -> &'static str {
    match kind {
        crate::tasks::TaskKind::Validate { .. } => "validate",
        crate::tasks::TaskKind::Backtest => "backtest",
        crate::tasks::TaskKind::Paper => "paper",
        crate::tasks::TaskKind::PaperRealtime => "paper(realtime)",
    }
}

// Runs are refreshed in `App` when entering the Reports view (or via `g`/F5).
