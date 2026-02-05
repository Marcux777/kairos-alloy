use crate::services::sentiment::{MissingValuePolicy, SentimentPoint, SentimentReport};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy)]
pub enum SentimentFormat {
    Csv,
    Json,
}

#[derive(Debug, Clone)]
pub struct SentimentQuery {
    pub path: PathBuf,
    pub format: SentimentFormat,
    pub missing_policy: MissingValuePolicy,
}

pub trait SentimentRepository {
    fn load_sentiment(
        &self,
        query: &SentimentQuery,
    ) -> Result<(Vec<SentimentPoint>, SentimentReport), String>;
}
