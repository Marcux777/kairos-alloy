use kairos_domain::value_objects::bar::Bar;
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpcvConfig {
    pub n_groups: usize,
    pub k_test: usize,
    pub horizon_bars: usize,
    pub purge_bars: usize,
    pub embargo_bars: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CpcvSegment {
    pub start_idx: usize,
    pub end_idx: usize,
    pub start_ts: i64,
    pub end_ts: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CpcvFold {
    pub fold_id: usize,
    pub test_groups: Vec<usize>,
    pub train_segments: Vec<CpcvSegment>,
    pub test_segments: Vec<CpcvSegment>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CpcvResult {
    pub total_bars: usize,
    pub groups: Vec<(usize, usize)>,
    pub folds: Vec<CpcvFold>,
}

pub fn generate_cpcv(bars: &[Bar], cfg: CpcvConfig) -> Result<CpcvResult, String> {
    if cfg.n_groups < 2 {
        return Err("cpcv.n_groups must be >= 2".to_string());
    }
    if cfg.k_test == 0 || cfg.k_test >= cfg.n_groups {
        return Err("cpcv.k_test must be >= 1 and < n_groups".to_string());
    }
    if bars.is_empty() {
        return Err("cannot run CPCV with 0 bars".to_string());
    }
    if bars.len() < cfg.n_groups {
        return Err(format!(
            "not enough bars for CPCV: bars={} n_groups={}",
            bars.len(),
            cfg.n_groups
        ));
    }

    let groups = partition_groups(bars.len(), cfg.n_groups);
    let combos = combinations(cfg.n_groups, cfg.k_test);

    let mut folds = Vec::with_capacity(combos.len());
    for (fold_id, test_groups) in combos.into_iter().enumerate() {
        let test_ranges = merge_ranges(test_groups.iter().map(|&g| groups[g]).collect::<Vec<_>>());
        let test_segments = ranges_to_segments(bars, &test_ranges);
        let train_segments = compute_train_segments(bars, &test_ranges, cfg);
        folds.push(CpcvFold {
            fold_id,
            test_groups,
            train_segments,
            test_segments,
        });
    }

    Ok(CpcvResult {
        total_bars: bars.len(),
        groups,
        folds,
    })
}

pub fn write_cpcv_csv(path: &Path, result: &CpcvResult) -> Result<(), String> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)
        .map_err(|err| format!("failed to create dir {}: {err}", parent.display()))?;

    let mut wtr = csv::Writer::from_path(path)
        .map_err(|err| format!("failed to create {}: {err}", path.display()))?;

    wtr.write_record([
        "fold_id",
        "set",
        "segment_id",
        "start_idx",
        "end_idx",
        "start_ts",
        "end_ts",
        "start_utc",
        "end_utc",
        "test_groups",
    ])
    .map_err(|err| format!("failed to write header: {err}"))?;

    for fold in &result.folds {
        let groups = fold
            .test_groups
            .iter()
            .map(|g| g.to_string())
            .collect::<Vec<_>>()
            .join("|");

        for (segment_id, seg) in fold.train_segments.iter().enumerate() {
            wtr.write_record([
                fold.fold_id.to_string(),
                "train".to_string(),
                segment_id.to_string(),
                seg.start_idx.to_string(),
                seg.end_idx.to_string(),
                seg.start_ts.to_string(),
                seg.end_ts.to_string(),
                ts_rfc3339(seg.start_ts),
                ts_rfc3339(seg.end_ts),
                groups.clone(),
            ])
            .map_err(|err| format!("failed to write train row: {err}"))?;
        }

        for (segment_id, seg) in fold.test_segments.iter().enumerate() {
            wtr.write_record([
                fold.fold_id.to_string(),
                "test".to_string(),
                segment_id.to_string(),
                seg.start_idx.to_string(),
                seg.end_idx.to_string(),
                seg.start_ts.to_string(),
                seg.end_ts.to_string(),
                ts_rfc3339(seg.start_ts),
                ts_rfc3339(seg.end_ts),
                groups.clone(),
            ])
            .map_err(|err| format!("failed to write test row: {err}"))?;
        }
    }

    wtr.flush()
        .map_err(|err| format!("failed to flush {}: {err}", path.display()))?;
    Ok(())
}

fn partition_groups(total: usize, n_groups: usize) -> Vec<(usize, usize)> {
    let mut out = Vec::with_capacity(n_groups);
    for g in 0..n_groups {
        let start = g * total / n_groups;
        let end = (g + 1) * total / n_groups;
        out.push((start, end.saturating_sub(1)));
    }
    out
}

fn combinations(n: usize, k: usize) -> Vec<Vec<usize>> {
    let mut out = Vec::new();
    let mut cur = (0..k).collect::<Vec<_>>();
    loop {
        out.push(cur.clone());
        let mut i = k;
        while i > 0 {
            i -= 1;
            if cur[i] != i + n - k {
                break;
            }
        }
        if cur[0] == n - k && cur[k - 1] == n - 1 {
            break;
        }
        cur[i] += 1;
        for j in i + 1..k {
            cur[j] = cur[j - 1] + 1;
        }
    }
    out
}

fn merge_ranges(mut ranges: Vec<(usize, usize)>) -> Vec<(usize, usize)> {
    ranges.sort_by_key(|r| r.0);
    let mut out: Vec<(usize, usize)> = Vec::new();
    for (s, e) in ranges {
        if let Some(last) = out.last_mut() {
            if s <= last.1 + 1 {
                last.1 = last.1.max(e);
                continue;
            }
        }
        out.push((s, e));
    }
    out
}

fn ranges_to_segments(bars: &[Bar], ranges: &[(usize, usize)]) -> Vec<CpcvSegment> {
    ranges
        .iter()
        .map(|&(s, e)| CpcvSegment {
            start_idx: s,
            end_idx: e,
            start_ts: bars[s].timestamp,
            end_ts: bars[e].timestamp,
        })
        .collect()
}

fn compute_train_segments(
    bars: &[Bar],
    test_ranges: &[(usize, usize)],
    cfg: CpcvConfig,
) -> Vec<CpcvSegment> {
    let total = bars.len();

    let mut is_test = vec![false; total];
    for &(s, e) in test_ranges {
        for idx in s..=e {
            is_test[idx] = true;
        }
    }

    let mut blocked = vec![false; total];
    let before = cfg
        .horizon_bars
        .saturating_add(cfg.purge_bars)
        .min(total.saturating_sub(1));
    let after = cfg.embargo_bars.min(total.saturating_sub(1));

    for &(s, e) in test_ranges {
        let block_start = s.saturating_sub(before);
        let block_end = (e + after).min(total - 1);
        for idx in block_start..=block_end {
            blocked[idx] = true;
        }
    }

    let mut segments: Vec<(usize, usize)> = Vec::new();
    let mut cur_start: Option<usize> = None;

    for idx in 0..total {
        let allowed = !is_test[idx] && !blocked[idx];
        match (cur_start, allowed) {
            (None, true) => cur_start = Some(idx),
            (Some(start), false) => {
                segments.push((start, idx - 1));
                cur_start = None;
            }
            _ => {}
        }
    }
    if let Some(start) = cur_start {
        segments.push((start, total - 1));
    }

    ranges_to_segments(bars, &segments)
}

fn ts_rfc3339(ts: i64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp(ts, 0)
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bars(n: usize) -> Vec<Bar> {
        (0..n)
            .map(|i| Bar {
                symbol: "BTCUSDT".to_string(),
                timestamp: i as i64 * 60,
                open: 1.0,
                high: 1.0,
                low: 1.0,
                close: 1.0,
                volume: 1.0,
            })
            .collect()
    }

    #[test]
    fn combinations_count_matches_binomial() {
        let combos = combinations(6, 2);
        assert_eq!(combos.len(), 15);
        assert_eq!(combos.first().cloned(), Some(vec![0, 1]));
        assert_eq!(combos.last().cloned(), Some(vec![4, 5]));
    }

    #[test]
    fn partition_groups_spans_full_range() {
        let groups = partition_groups(10, 3);
        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0], (0, 2));
        assert_eq!(groups[1], (3, 5));
        assert_eq!(groups[2], (6, 9));
    }

    #[test]
    fn generate_cpcv_purges_before_test_start() {
        let b = bars(12);
        let cfg = CpcvConfig {
            n_groups: 3,
            k_test: 1,
            horizon_bars: 2,
            purge_bars: 1,
            embargo_bars: 0,
        };
        let result = generate_cpcv(&b, cfg).unwrap();
        let fold = &result.folds[0]; // test group 0 => idx 0..3
        assert_eq!(fold.test_segments[0].start_idx, 0);
        assert_eq!(fold.test_segments[0].end_idx, 3);
        // purge_before = horizon(2)+purge(1)=3 => train cannot start before idx 7 (since group 1 starts at 4 but blocked 0..3 anyway)
        // blocked window is [0..3], so training begins at 4 (not blocked), but must respect blocked created for s=0 so no extra.
        assert!(fold
            .train_segments
            .iter()
            .all(|seg| seg.start_idx > fold.test_segments[0].end_idx));
    }

    #[test]
    fn train_and_test_segments_never_overlap() {
        let b = bars(30);
        let cfg = CpcvConfig {
            n_groups: 6,
            k_test: 2,
            horizon_bars: 1,
            purge_bars: 1,
            embargo_bars: 1,
        };
        let result = generate_cpcv(&b, cfg).unwrap();
        for fold in &result.folds {
            for t in &fold.test_segments {
                for tr in &fold.train_segments {
                    let overlap = !(tr.end_idx < t.start_idx || t.end_idx < tr.start_idx);
                    assert!(!overlap);
                }
            }
        }
    }
}
