use kairos_tui::{logging, TuiOpts};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

fn main() {
    let log_store = Arc::new(parking_lot::Mutex::new(logging::LogStore::new(5000)));
    if let Err(err) = init_tracing(log_store.clone()) {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
    if let Err(err) = init_metrics() {
        eprintln!("error: {err}");
        std::process::exit(1);
    }

    let initial_config_path = std::env::var("KAIROS_CONFIG")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .map(PathBuf::from);

    let opts = TuiOpts {
        initial_config_path,
        log_store,
        default_out_dir: PathBuf::from("runs"),
    };

    if let Err(err) = kairos_tui::run(opts) {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

fn init_tracing(log_store: Arc<parking_lot::Mutex<logging::LogStore>>) -> Result<(), String> {
    let filter = std::env::var("KAIROS_LOG").unwrap_or_else(|_| "info".to_string());
    let env_filter = tracing_subscriber::EnvFilter::try_new(filter)
        .map_err(|err| format!("invalid log filter: {err}"))?;

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_writer(logging::LogMakeWriter::new(log_store))
        .init();

    Ok(())
}

#[cfg(feature = "prometheus")]
fn init_metrics() -> Result<Option<SocketAddr>, String> {
    use metrics_exporter_prometheus::PrometheusBuilder;

    let Some(raw) = std::env::var("KAIROS_METRICS_ADDR").ok() else {
        return Ok(None);
    };
    if raw.trim().is_empty() {
        return Ok(None);
    }

    let addr: SocketAddr = raw
        .parse()
        .map_err(|err| format!("invalid KAIROS_METRICS_ADDR (expected host:port): {err}"))?;

    PrometheusBuilder::new()
        .with_http_listener(addr)
        .install()
        .map_err(|err| format!("failed to install prometheus exporter: {err}"))?;

    tracing::info!(metrics_addr = %addr, "prometheus metrics exporter enabled");
    Ok(Some(addr))
}

#[cfg(not(feature = "prometheus"))]
fn init_metrics() -> Result<Option<SocketAddr>, String> {
    Ok(None)
}
