#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Timeframe {
    pub label: String,
    pub step_seconds: i64,
}

impl Timeframe {
    pub fn parse(value: &str) -> Result<Self, String> {
        let normalized = value.trim().to_lowercase();
        let label = match normalized.as_str() {
            "1m" | "1min" => "1min",
            "3m" | "3min" => "3min",
            "5m" | "5min" => "5min",
            "15m" | "15min" => "15min",
            "30m" | "30min" => "30min",
            "1h" | "1hour" => "1hour",
            "2h" | "2hour" => "2hour",
            "4h" | "4hour" => "4hour",
            "6h" | "6hour" => "6hour",
            "8h" | "8hour" => "8hour",
            "12h" | "12hour" => "12hour",
            "1d" | "1day" => "1day",
            "1w" | "1week" => "1week",
            "1mo" | "1month" => "1month",
            _ => return Err(format!("unsupported timeframe: {value}")),
        };

        let step_seconds = parse_duration_like_seconds(label)?;
        Ok(Self {
            label: label.to_string(),
            step_seconds,
        })
    }

    pub fn parse_seconds(value: &str) -> Result<Self, String> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err("empty timeframe".to_string());
        }
        let seconds: i64 = trimmed
            .parse()
            .map_err(|_| format!("invalid timeframe seconds: {value}"))?;
        if seconds <= 0 {
            return Err(format!("invalid timeframe seconds: {value}"));
        }
        Ok(Self {
            label: trimmed.to_string(),
            step_seconds: seconds,
        })
    }

    pub fn parse_or_seconds(value: &str) -> Result<Self, String> {
        Self::parse(value).or_else(|_| Self::parse_seconds(value))
    }
}

pub fn parse_duration_like_seconds(value: &str) -> Result<i64, String> {
    let trimmed = value.trim().to_lowercase();
    if trimmed.is_empty() {
        return Err("empty duration".to_string());
    }
    if let Ok(seconds) = trimmed.parse::<i64>() {
        return Ok(seconds);
    }

    let (number_part, unit) = if let Some(stripped) = trimmed.strip_suffix("min") {
        (stripped, "min")
    } else if let Some(stripped) = trimmed.strip_suffix("hour") {
        (stripped, "hour")
    } else if let Some(stripped) = trimmed.strip_suffix("day") {
        (stripped, "day")
    } else if let Some(stripped) = trimmed.strip_suffix("week") {
        (stripped, "week")
    } else if let Some(stripped) = trimmed.strip_suffix("month") {
        (stripped, "month")
    } else {
        let (number_part, unit) = trimmed.split_at(trimmed.len().saturating_sub(1));
        (number_part, unit)
    };

    let multiplier = match unit {
        "s" => 1,
        "m" | "min" => 60,
        "h" | "hour" => 3600,
        "d" | "day" => 86400,
        "w" | "week" => 604800,
        "mo" | "month" => 2592000,
        _ => return Err(format!("unsupported duration unit: {unit}")),
    };

    let number: i64 = number_part
        .parse()
        .map_err(|_| format!("invalid duration: {value}"))?;
    Ok(number * multiplier)
}

