use crate::services::ohlcv::DataQualityReport;
use crate::value_objects::bar::Bar;

#[derive(Debug, Clone)]
pub struct OhlcvQuery {
    pub exchange: String,
    pub market: String,
    pub symbol: String,
    pub timeframe: String,
    pub expected_step_seconds: Option<i64>,
}

pub trait MarketDataRepository {
    fn load_ohlcv(&self, query: &OhlcvQuery) -> Result<(Vec<Bar>, DataQualityReport), String>;
}
