use std::{collections::BTreeMap, process::Command, thread};

use chrono::{Datelike, NaiveDate};
use serde_json::Value;

use crate::models::{DailyUsage, UsageDataset};

const API_VERSION: &str = "2022-11-28";
const RECENT_WINDOW_DAYS: u32 = 14;
// Default `--full` tops out at 21 requests, so 20 keeps us near one wave
// while staying well below GitHub's 100-concurrent-request secondary limit.
const MAX_CONCURRENT_REQUESTS: usize = 20;

#[derive(Clone, Debug)]
struct ApiRecord {
    date: NaiveDate,
    quantity: f64,
    model: Option<String>,
    total_monthly_quota: Option<f64>,
    exceeds_quota: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum FetchTarget {
    RecentDay(NaiveDate),
    CurrentMonth(NaiveDate),
    CurrentDay(NaiveDate),
    PreviousMonth(NaiveDate),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FetchRequest {
    target: FetchTarget,
    year: i32,
    month: u32,
    day: Option<u32>,
    synthetic_date: NaiveDate,
}

#[derive(Clone, Debug)]
struct FetchResponse {
    request: FetchRequest,
    rows: Vec<ApiRecord>,
}

pub fn load_api_usage(
    username: &str,
    fallback_quota: u32,
    previous_months: usize,
    today: NaiveDate,
    include_recent_daily: bool,
    include_previous_months: bool,
) -> Result<UsageDataset, String> {
    let mut by_day: BTreeMap<NaiveDate, DailyUsage> = BTreeMap::new();
    let mut latest_quota: Option<(NaiveDate, f64)> = None;
    let mut monthly_totals: BTreeMap<NaiveDate, f64> = BTreeMap::new();
    let mut current_month_total = None;
    let mut current_day_total = None;
    let mut current_month_model_breakdown = BTreeMap::new();

    let recent_window_starts_at_month_start =
        include_recent_daily && recent_window_start_day(today) == 1;
    let mut recent_rows = Vec::new();

    for response in execute_requests(
        username,
        build_fetch_plan(
            today,
            previous_months,
            include_recent_daily,
            include_previous_months,
        ),
    )? {
        let rows = response.rows;
        match response.request.target {
            FetchTarget::RecentDay(date) => {
                if date == today {
                    current_day_total = Some(sum_records(&rows));
                }
                recent_rows.extend(rows.iter().cloned());
                merge_records(&mut by_day, &mut latest_quota, rows);
            }
            FetchTarget::CurrentMonth(_) => {
                observe_quota(&mut latest_quota, &rows);
                current_month_total = Some(sum_records(&rows));
                current_month_model_breakdown = aggregate_models(&rows);
            }
            FetchTarget::CurrentDay(_) => {
                observe_quota(&mut latest_quota, &rows);
                current_day_total = Some(sum_records(&rows));
            }
            FetchTarget::PreviousMonth(month_anchor) => {
                observe_quota(&mut latest_quota, &rows);
                monthly_totals.insert(month_anchor, sum_records(&rows));
            }
        }
    }

    if recent_window_starts_at_month_start {
        current_month_total = Some(sum_records(&recent_rows));
        current_month_model_breakdown = aggregate_models(&recent_rows);
    }

    Ok(UsageDataset {
        daily_usage: by_day.into_values().collect(),
        monthly_totals,
        current_month_total,
        current_day_total,
        current_month_model_breakdown,
        show_model_breakdown: include_previous_months,
        quota: latest_quota
            .map(|(_, quota)| quota)
            .unwrap_or(fallback_quota as f64),
    })
}

fn build_fetch_plan(
    today: NaiveDate,
    previous_months: usize,
    include_recent_daily: bool,
    include_previous_months: bool,
) -> Vec<FetchRequest> {
    let mut requests = Vec::new();
    let month_start = crate::analytics::first_day_of_month(today);

    if include_recent_daily {
        let start_day = recent_window_start_day(today);
        for day in start_day..=today.day() {
            let date =
                NaiveDate::from_ymd_opt(today.year(), today.month(), day).expect("valid date");
            requests.push(FetchRequest::for_day(FetchTarget::RecentDay(date), date));
        }

        if start_day != 1 {
            requests.push(FetchRequest::for_month(
                FetchTarget::CurrentMonth(month_start),
                month_start,
            ));
        }
    } else {
        requests.push(FetchRequest::for_month(
            FetchTarget::CurrentMonth(month_start),
            month_start,
        ));
        requests.push(FetchRequest::for_day(FetchTarget::CurrentDay(today), today));
    }

    if include_previous_months {
        for offset in 1..=previous_months {
            let month_anchor = month_start
                .checked_sub_months(chrono::Months::new(offset as u32))
                .expect("valid previous month");
            requests.push(FetchRequest::for_month(
                FetchTarget::PreviousMonth(month_anchor),
                month_anchor,
            ));
        }
    }

    requests
}

fn recent_window_start_day(today: NaiveDate) -> u32 {
    today
        .day()
        .saturating_sub(RECENT_WINDOW_DAYS.saturating_sub(1))
        .max(1)
}

fn execute_requests(
    username: &str,
    requests: Vec<FetchRequest>,
) -> Result<Vec<FetchResponse>, String> {
    let mut responses = Vec::with_capacity(requests.len());
    let username = username.to_string();

    for chunk in requests.chunks(MAX_CONCURRENT_REQUESTS) {
        let chunk_results = thread::scope(|scope| {
            let handles: Vec<_> = chunk
                .iter()
                .map(|request| {
                    let username = username.clone();
                    scope.spawn(move || -> Result<FetchResponse, String> {
                        let rows = fetch_period(
                            &username,
                            request.year,
                            request.month,
                            request.day,
                            request.synthetic_date,
                        )?;
                        Ok(FetchResponse {
                            request: request.clone(),
                            rows,
                        })
                    })
                })
                .collect();

            handles
                .into_iter()
                .map(|handle| {
                    handle
                        .join()
                        .map_err(|_| "internal error: GitHub request worker panicked".to_string())?
                })
                .collect::<Result<Vec<_>, String>>()
        })?;

        responses.extend(chunk_results);
    }

    Ok(responses)
}

impl FetchRequest {
    fn for_day(target: FetchTarget, date: NaiveDate) -> Self {
        Self {
            target,
            year: date.year(),
            month: date.month(),
            day: Some(date.day()),
            synthetic_date: date,
        }
    }

    fn for_month(target: FetchTarget, month_start: NaiveDate) -> Self {
        Self {
            target,
            year: month_start.year(),
            month: month_start.month(),
            day: None,
            synthetic_date: month_start,
        }
    }
}

fn fetch_period(
    username: &str,
    year: i32,
    month: u32,
    day: Option<u32>,
    synthetic_date: NaiveDate,
) -> Result<Vec<ApiRecord>, String> {
    let period_label = describe_period(year, month, day);
    let mut endpoint = format!(
        "/users/{username}/settings/billing/premium_request/usage?year={year}&month={month}"
    );
    if let Some(day) = day {
        endpoint.push_str(&format!("&day={day}"));
    }

    let output = Command::new("gh")
        .args([
            "api",
            "-H",
            "Accept: application/vnd.github+json",
            "-H",
            &format!("X-GitHub-Api-Version: {API_VERSION}"),
            &endpoint,
        ])
        .output()
        .map_err(|error| {
            format!("failed to run gh for {period_label}: {error}. Is GitHub CLI installed?")
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let message = stderr.trim();
        if message.contains("HTTP 401") || message.to_ascii_lowercase().contains("authentication") {
            return Err(
                "gh is not authenticated for the billing API. Run `gh auth login` and try again."
                    .to_string(),
            );
        }
        if message.contains("needs the \"user\" scope") {
            return Err(
                "gh is logged in but missing the required `user` scope. Run `gh auth refresh -h github.com -s user` and try again."
                    .to_string(),
            );
        }
        if message.contains("HTTP 404") {
            return Err(
                "GitHub's billing API endpoint was not available for this account. Check that your Copilot usage is billed to your personal account and that `gh` has the required scope."
                    .to_string(),
            );
        }
        if message.to_ascii_lowercase().contains("rate limit") {
            return Err("GitHub API rate limit exceeded. Try again later.".to_string());
        }
        return Err(format!(
            "gh api request failed for {period_label}: {}",
            if message.is_empty() {
                "unknown error"
            } else {
                message
            }
        ));
    }

    let value = serde_json::from_slice::<Value>(&output.stdout).map_err(|error| {
        format!("failed to parse GitHub API response for {period_label}: {error}")
    })?;
    Ok(parse_usage_payload(&value, synthetic_date))
}

fn describe_period(year: i32, month: u32, day: Option<u32>) -> String {
    match day {
        Some(day) => format!("{year}-{month:02}-{day:02}"),
        None => format!("{year}-{month:02}"),
    }
}

fn merge_records(
    by_day: &mut BTreeMap<NaiveDate, DailyUsage>,
    latest_quota: &mut Option<(NaiveDate, f64)>,
    records: Vec<ApiRecord>,
) {
    for row in records {
        let entry = by_day
            .entry(row.date)
            .or_insert_with(|| DailyUsage::new(row.date));
        entry.add_quantity(
            row.quantity,
            row.model.as_deref(),
            row.total_monthly_quota,
            row.exceeds_quota,
        );

        if let Some(quota) = row.total_monthly_quota.filter(|value| *value > 0.0) {
            match latest_quota {
                Some((existing_date, _)) if *existing_date > row.date => {}
                _ => *latest_quota = Some((row.date, quota)),
            }
        }
    }
}

fn observe_quota(latest_quota: &mut Option<(NaiveDate, f64)>, records: &[ApiRecord]) {
    for row in records {
        if let Some(quota) = row.total_monthly_quota.filter(|value| *value > 0.0) {
            match latest_quota {
                Some((existing_date, _)) if *existing_date > row.date => {}
                _ => *latest_quota = Some((row.date, quota)),
            }
        }
    }
}

fn sum_records(records: &[ApiRecord]) -> f64 {
    records.iter().map(|row| row.quantity).sum()
}

fn aggregate_models(records: &[ApiRecord]) -> BTreeMap<String, f64> {
    let mut models = BTreeMap::new();
    for row in records {
        if let Some(model) = row.model.as_ref() {
            *models.entry(model.clone()).or_insert(0.0) += row.quantity;
        }
    }
    models
}

fn parse_usage_payload(value: &Value, synthetic_date: NaiveDate) -> Vec<ApiRecord> {
    let mut records = Vec::new();
    collect_records(value, synthetic_date, &mut records);
    records
}

fn collect_records(value: &Value, synthetic_date: NaiveDate, output: &mut Vec<ApiRecord>) {
    match value {
        Value::Array(items) => {
            for item in items {
                collect_records(item, synthetic_date, output);
            }
        }
        Value::Object(map) => {
            let mut found_nested = false;
            for key in [
                "data",
                "usage",
                "items",
                "usageItems",
                "results",
                "entries",
                "days",
                "models",
            ] {
                if let Some(child) = map.get(key) {
                    match child {
                        Value::Array(_) | Value::Object(_) => {
                            found_nested = true;
                            collect_records(child, synthetic_date, output);
                        }
                        _ => {}
                    }
                }
            }

            if !found_nested && let Some(record) = parse_row(map, synthetic_date) {
                output.push(record);
            }
        }
        _ => {}
    }
}

fn parse_row(map: &serde_json::Map<String, Value>, synthetic_date: NaiveDate) -> Option<ApiRecord> {
    let quantity = extract_f64(
        map,
        &[
            "grossQuantity",
            "quantity",
            "gross_quantity",
            "total",
            "used",
            "requests",
            "premium_requests",
            "total_requests",
            "count",
        ],
    )?;

    Some(ApiRecord {
        date: parse_date(map, synthetic_date),
        quantity,
        model: extract_string(map, &["model", "model_name", "name"]),
        total_monthly_quota: extract_f64(map, &["total_monthly_quota", "monthly_quota", "quota"]),
        exceeds_quota: extract_bool(map, &["exceeds_quota", "over_quota"]).unwrap_or(false),
    })
}

fn parse_date(map: &serde_json::Map<String, Value>, fallback: NaiveDate) -> NaiveDate {
    if let Some(raw_date) = extract_string(map, &["date", "day"])
        && let Ok(date) = NaiveDate::parse_from_str(&raw_date, "%Y-%m-%d")
    {
        return date;
    }

    let year = extract_u32(map, &["year"]).map(|value| value as i32);
    let month = extract_u32(map, &["month"]);
    let day = extract_u32(map, &["day"]).or(Some(1));
    if let (Some(year), Some(month), Some(day)) = (year, month, day)
        && let Some(date) = NaiveDate::from_ymd_opt(year, month, day)
    {
        return date;
    }

    fallback
}

fn extract_u32(map: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<u32> {
    extract_f64(map, keys).and_then(|value| {
        if value.is_sign_negative() {
            None
        } else {
            Some(value.round() as u32)
        }
    })
}

fn extract_f64(map: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<f64> {
    keys.iter()
        .find_map(|key| map.get(*key))
        .and_then(value_to_f64)
}

fn extract_string(map: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        map.get(*key).and_then(|value| match value {
            Value::String(text) if !text.trim().is_empty() => Some(text.trim().to_string()),
            _ => None,
        })
    })
}

fn extract_bool(map: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<bool> {
    keys.iter().find_map(|key| {
        map.get(*key).and_then(|value| match value {
            Value::Bool(flag) => Some(*flag),
            Value::String(text) if !text.trim().is_empty() => text.parse::<bool>().ok(),
            _ => None,
        })
    })
}

fn value_to_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64().filter(|value| !value.is_sign_negative()),
        Value::String(text) if !text.trim().is_empty() => text.trim().parse::<f64>().ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;
    use serde_json::json;

    use super::{FetchTarget, build_fetch_plan, parse_usage_payload};

    #[test]
    fn parses_gross_quantity_from_usage_items() {
        let payload = json!({
            "timePeriod": { "year": 2026, "month": 3, "day": 11 },
            "usageItems": [
                {
                    "model": "Claude Opus 4.6",
                    "grossQuantity": 18.0,
                    "netQuantity": 0.0
                },
                {
                    "model": "Claude Haiku 4.5",
                    "grossQuantity": 0.33,
                    "netQuantity": 0.0
                }
            ]
        });

        let rows = parse_usage_payload(&payload, NaiveDate::from_ymd_opt(2026, 3, 11).unwrap());
        assert_eq!(rows.len(), 2);
        assert!((rows[0].quantity - 18.0).abs() < 0.001);
        assert!((rows[1].quantity - 0.33).abs() < 0.001);
    }

    #[test]
    fn full_plan_skips_redundant_current_period_requests_when_recent_window_covers_month() {
        let today = NaiveDate::from_ymd_opt(2026, 3, 11).unwrap();

        let plan = build_fetch_plan(today, 2, true, true);

        assert_eq!(plan.len(), 13);
        assert_eq!(
            plan.iter()
                .filter(|request| matches!(request.target, FetchTarget::RecentDay(_)))
                .count(),
            11
        );
        assert_eq!(
            plan.iter()
                .filter(|request| matches!(request.target, FetchTarget::PreviousMonth(_)))
                .count(),
            2
        );
        assert!(!plan.iter().any(|request| matches!(
            request.target,
            FetchTarget::CurrentMonth(_) | FetchTarget::CurrentDay(_)
        )));
    }

    #[test]
    fn compact_plan_fetches_current_month_and_today() {
        let today = NaiveDate::from_ymd_opt(2026, 3, 20).unwrap();

        let plan = build_fetch_plan(today, 0, false, false);

        assert_eq!(plan.len(), 2);
        assert!(
            plan.iter()
                .any(|request| matches!(request.target, FetchTarget::CurrentMonth(_)))
        );
        assert!(
            plan.iter()
                .any(|request| matches!(request.target, FetchTarget::CurrentDay(_)))
        );
    }

    #[test]
    fn full_plan_adds_current_month_when_recent_window_is_partial() {
        let today = NaiveDate::from_ymd_opt(2026, 3, 20).unwrap();

        let plan = build_fetch_plan(today, 0, true, false);

        assert_eq!(plan.len(), 15);
        assert_eq!(
            plan.iter()
                .filter(|request| matches!(request.target, FetchTarget::RecentDay(_)))
                .count(),
            14
        );
        assert!(
            plan.iter()
                .any(|request| matches!(request.target, FetchTarget::CurrentMonth(_)))
        );
        assert!(
            !plan
                .iter()
                .any(|request| matches!(request.target, FetchTarget::CurrentDay(_)))
        );
    }
}
