pub use kairos_domain::value_objects::timeframe::*;

#[cfg(test)]
mod tests {
    use super::{parse_duration_like_seconds, Timeframe};

    #[test]
    fn parses_timeframe_aliases() {
        assert_eq!(Timeframe::parse("1m").unwrap().label, "1min");
        assert_eq!(Timeframe::parse("1hour").unwrap().label, "1hour");
        assert_eq!(Timeframe::parse("1d").unwrap().label, "1day");
        assert_eq!(Timeframe::parse("1mo").unwrap().label, "1month");
    }

    #[test]
    fn parses_duration_like_seconds() {
        assert_eq!(parse_duration_like_seconds("5s").unwrap(), 5);
        assert_eq!(parse_duration_like_seconds("2m").unwrap(), 120);
        assert_eq!(parse_duration_like_seconds("1h").unwrap(), 3600);
        assert_eq!(parse_duration_like_seconds("1min").unwrap(), 60);
    }

    #[test]
    fn parses_numeric_seconds_timeframe() {
        let tf = Timeframe::parse_or_seconds("60").unwrap();
        assert_eq!(tf.label, "60");
        assert_eq!(tf.step_seconds, 60);
    }
}
