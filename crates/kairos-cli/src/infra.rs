use crate::config::{AgentMode, Config};
use kairos_domain::repositories::agent::AgentClient as AgentPort;
use kairos_domain::repositories::artifacts::{ArtifactReader, ArtifactWriter};
use kairos_domain::repositories::market_data::MarketDataRepository;
use kairos_domain::repositories::sentiment::SentimentRepository;
use kairos_infrastructure::agents::AgentClient as InfraAgentClient;
use kairos_infrastructure::artifacts::{FilesystemArtifactReader, FilesystemArtifactWriter};
use kairos_infrastructure::persistence::postgres_ohlcv::PostgresMarketDataRepository;
use kairos_infrastructure::sentiment::FilesystemSentimentRepository;
use std::env;

pub struct EngineDeps {
    pub market_data: Box<dyn MarketDataRepository>,
    pub sentiment_repo: Box<dyn SentimentRepository>,
    pub artifacts: Box<dyn ArtifactWriter>,
    pub remote_agent: Option<Box<dyn AgentPort>>,
}

pub struct ValidateDeps {
    pub market_data: Box<dyn MarketDataRepository>,
    pub sentiment_repo: Box<dyn SentimentRepository>,
}

pub struct ReportingDeps {
    pub reader: Box<dyn ArtifactReader>,
    pub writer: Box<dyn ArtifactWriter>,
}

pub fn build_engine_deps(config: &Config) -> Result<EngineDeps, String> {
    Ok(EngineDeps {
        market_data: build_market_data_repo(config)?,
        sentiment_repo: Box::new(FilesystemSentimentRepository),
        artifacts: Box::new(FilesystemArtifactWriter::new()),
        remote_agent: build_remote_agent(config)?,
    })
}

pub fn build_validate_deps(config: &Config) -> Result<ValidateDeps, String> {
    Ok(ValidateDeps {
        market_data: build_market_data_repo(config)?,
        sentiment_repo: Box::new(FilesystemSentimentRepository),
    })
}

pub fn build_reporting_deps() -> ReportingDeps {
    ReportingDeps {
        reader: Box::new(FilesystemArtifactReader::new()),
        writer: Box::new(FilesystemArtifactWriter::new()),
    }
}

fn resolve_db_url(config: &Config) -> Result<String, String> {
    match config.db.url.as_deref() {
        Some(url) if !url.trim().is_empty() => Ok(url.to_string()),
        _ => env::var("KAIROS_DB_URL")
            .map_err(|_| "missing db.url in config and env KAIROS_DB_URL is not set".to_string()),
    }
}

fn build_market_data_repo(config: &Config) -> Result<Box<dyn MarketDataRepository>, String> {
    let db_url = resolve_db_url(config)?;
    let pool_max_size = config.db.pool_max_size.unwrap_or(8);
    Ok(Box::new(PostgresMarketDataRepository::new(
        db_url,
        config.db.ohlcv_table.to_string(),
        pool_max_size,
    )?))
}

fn build_remote_agent(config: &Config) -> Result<Option<Box<dyn AgentPort>>, String> {
    match config.agent.mode {
        AgentMode::Remote => {
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
