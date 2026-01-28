use clap::{Parser, Subcommand};
use kairos_ingest::{ingest_kucoin, migrate_db, Market};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "kairos-ingest")]
#[command(about = "KuCoin OHLCV ingestion into PostgreSQL.", version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Apply SQL migrations to the target database.
    Migrate {
        #[arg(long)]
        db_url: String,
        #[arg(long, default_value = "migrations/0001_create_ohlcv_candles.sql")]
        migrations_path: PathBuf,
    },
    /// Ingest KuCoin OHLCV into PostgreSQL.
    IngestKucoin {
        #[arg(long)]
        db_url: String,
        #[arg(long)]
        symbol: String,
        #[arg(long, default_value = "spot")]
        market: Market,
        #[arg(long, default_value = "1min")]
        timeframe: String,
        #[arg(long)]
        start: String,
        #[arg(long)]
        end: Option<String>,
        #[arg(long, default_value = "kucoin")]
        exchange: String,
        #[arg(long, default_value = "kucoin")]
        source: String,
        #[arg(long, default_value_t = 350)]
        sleep_ms: u64,
        #[arg(long, default_value_t = 500)]
        batch_size: usize,
        /// Override KuCoin base URL (useful for tests; defaults to real KuCoin endpoints).
        #[arg(long)]
        base_url: Option<String>,
    },
}

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), String> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Migrate {
            db_url,
            migrations_path,
        } => migrate_db(&db_url, migrations_path.as_path()).await,
        Commands::IngestKucoin {
            db_url,
            symbol,
            market,
            timeframe,
            start,
            end,
            exchange,
            source,
            sleep_ms,
            batch_size,
            base_url,
        } => {
            ingest_kucoin(
                &db_url,
                &symbol,
                market,
                &timeframe,
                &start,
                end.as_deref(),
                &exchange,
                &source,
                sleep_ms,
                batch_size,
                base_url.as_deref(),
            )
            .await
        }
    }
}

