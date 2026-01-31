use std::net::SocketAddr;

pub fn init_tracing(log_level: &str, log_format: &str) -> Result<(), String> {
    let filter = std::env::var("KAIROS_LOG").unwrap_or_else(|_| log_level.to_string());
    let env_filter = tracing_subscriber::EnvFilter::try_new(filter)
        .map_err(|err| format!("invalid log filter: {err}"))?;

    let format = log_format.trim().to_lowercase();
    if format == "json" {
        tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .json()
            .init();
        return Ok(());
    }

    tracing_subscriber::fmt().with_env_filter(env_filter).init();
    Ok(())
}

#[cfg(feature = "prometheus")]
pub fn init_metrics(metrics_addr: Option<&str>) -> Result<Option<SocketAddr>, String> {
    use metrics_exporter_prometheus::PrometheusBuilder;

    let Some(raw) = metrics_addr else {
        return Ok(None);
    };
    let addr: SocketAddr = raw
        .parse()
        .map_err(|err| format!("invalid --metrics-addr (expected host:port): {err}"))?;

    PrometheusBuilder::new()
        .with_http_listener(addr)
        .install()
        .map_err(|err| format!("failed to install prometheus exporter: {err}"))?;

    tracing::info!(metrics_addr = %addr, "prometheus metrics exporter enabled");
    Ok(Some(addr))
}

#[cfg(not(feature = "prometheus"))]
pub fn init_metrics(metrics_addr: Option<&str>) -> Result<Option<SocketAddr>, String> {
    if metrics_addr.is_some() {
        return Err("metrics exporter requires kairos-cli feature `prometheus`".to_string());
    }
    Ok(None)
}
