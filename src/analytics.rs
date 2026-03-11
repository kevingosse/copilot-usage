use std::collections::HashMap;

use chrono::{Datelike, Duration, Months, NaiveDate};

use crate::models::{MonthlyUsage, UsageDataset, UsageSummary};

pub fn analyze_usage(
    dataset: &UsageDataset,
    today: NaiveDate,
    previous_months: usize,
) -> UsageSummary {
    let month_start = first_day_of_month(today);
    let total_days_in_month = days_in_month(today);

    let mut by_day: HashMap<NaiveDate, f64> = HashMap::new();
    let mut current_month_models = dataset.current_month_model_breakdown.clone();
    let mut month_totals = dataset.monthly_totals.clone();
    let explicit_current_month_models = !current_month_models.is_empty();
    let mut model_breakdown_available = explicit_current_month_models;

    for entry in &dataset.daily_usage {
        *by_day.entry(entry.date).or_insert(0.0) += entry.total_requests;

        let entry_month = first_day_of_month(entry.date);
        if !dataset.monthly_totals.contains_key(&entry_month) {
            *month_totals.entry(entry_month).or_insert(0.0) += entry.total_requests;
        }

        if !explicit_current_month_models && entry_month == month_start {
            if !entry.model_breakdown.is_empty() {
                model_breakdown_available = true;
            }
            for (model, quantity) in &entry.model_breakdown {
                *current_month_models.entry(model.clone()).or_insert(0.0) += quantity;
            }
        }
    }

    let days_elapsed = today.day();
    let days_remaining = total_days_in_month.saturating_sub(days_elapsed);
    let mtd_used = dataset.current_month_total.unwrap_or_else(|| {
        (1..=days_elapsed)
            .map(|day| {
                let date = NaiveDate::from_ymd_opt(today.year(), today.month(), day)
                    .expect("valid current month date");
                by_day.get(&date).copied().unwrap_or(0.0)
            })
            .sum::<f64>()
    });

    let remaining_quota = (dataset.quota - mtd_used).max(0.0);
    let percent_consumed = if dataset.quota == 0.0 {
        0.0
    } else {
        (mtd_used as f64 / dataset.quota as f64) * 100.0
    };

    let avg_daily_usage = if days_elapsed == 0 {
        0.0
    } else {
        mtd_used / days_elapsed as f64
    };
    let today_used = dataset
        .current_day_total
        .unwrap_or_else(|| by_day.get(&today).copied().unwrap_or(0.0));
    let projected_total = avg_daily_usage * total_days_in_month as f64;
    let projected_over_quota = projected_total > dataset.quota;
    let cushion_or_overshoot = dataset.quota - projected_total;
    let projected_run_out_date = if projected_over_quota && avg_daily_usage > 0.0 {
        let days_to_quota = (dataset.quota / avg_daily_usage).ceil().max(1.0) as i64;
        Some(
            month_start
                .checked_add_signed(Duration::days(days_to_quota - 1))
                .unwrap_or(today),
        )
    } else {
        None
    };

    let daily_budget = if days_remaining == 0 {
        None
    } else {
        Some(remaining_quota / days_remaining as f64)
    };
    let recent_days = if by_day.is_empty() {
        Vec::new()
    } else {
        let recent_window = 14_i64;
        (0..recent_window)
            .map(|offset| today - Duration::days(recent_window - 1 - offset))
            .map(|date| (date, by_day.get(&date).copied().unwrap_or(0.0)))
            .collect::<Vec<_>>()
    };

    let previous = if month_totals.is_empty() {
        Vec::new()
    } else {
        let mut previous = Vec::with_capacity(previous_months);
        for offset in (1..=previous_months).rev() {
            let previous_month_start = month_start
                .checked_sub_months(Months::new(offset as u32))
                .expect("valid previous month");
            previous.push(MonthlyUsage {
                month_start: previous_month_start,
                total_requests: month_totals
                    .get(&previous_month_start)
                    .copied()
                    .unwrap_or(0.0),
            });
        }
        previous
    };

    let mut model_breakdown = current_month_models.into_iter().collect::<Vec<_>>();
    model_breakdown.sort_by(|left, right| {
        right
            .1
            .partial_cmp(&left.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.0.cmp(&right.0))
    });

    UsageSummary {
        month_start,
        today,
        quota: dataset.quota,
        mtd_used,
        remaining_quota,
        percent_consumed,
        days_elapsed,
        days_remaining,
        avg_daily_usage,
        today_used,
        projected_over_quota,
        cushion_or_overshoot,
        projected_run_out_date,
        daily_budget,
        recent_days,
        previous_months: previous,
        model_breakdown,
        model_breakdown_available,
        show_model_breakdown: dataset.show_model_breakdown,
    }
}

pub fn first_day_of_month(date: NaiveDate) -> NaiveDate {
    NaiveDate::from_ymd_opt(date.year(), date.month(), 1).expect("valid first day of month")
}

pub fn days_in_month(date: NaiveDate) -> u32 {
    let next_month = if date.month() == 12 {
        NaiveDate::from_ymd_opt(date.year() + 1, 1, 1).expect("valid date")
    } else {
        NaiveDate::from_ymd_opt(date.year(), date.month() + 1, 1).expect("valid date")
    };
    (next_month - first_day_of_month(date)).num_days() as u32
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use chrono::NaiveDate;

    use crate::models::{DailyUsage, UsageDataset};

    use super::analyze_usage;

    #[test]
    fn computes_projection_and_budget() {
        let today = NaiveDate::from_ymd_opt(2026, 3, 10).unwrap();
        let mut entries = Vec::new();
        for day in 1..=10 {
            let date = NaiveDate::from_ymd_opt(2026, 3, day).unwrap();
            let mut usage = DailyUsage::new(date);
            usage.add_quantity(30.0, Some("gpt-4o"), Some(1_500.0), false);
            entries.push(usage);
        }

        let summary = analyze_usage(
            &UsageDataset {
                daily_usage: entries,
                monthly_totals: BTreeMap::new(),
                current_month_total: None,
                current_day_total: None,
                current_month_model_breakdown: BTreeMap::new(),
                show_model_breakdown: false,
                quota: 1_500.0,
            },
            today,
            6,
        );

        assert_eq!(summary.mtd_used, 300.0);
        assert_eq!(summary.remaining_quota, 1_200.0);
        assert_eq!(summary.days_elapsed, 10);
        assert!(summary.daily_budget.unwrap() > 50.0);
        assert!(!summary.projected_over_quota);
    }

    #[test]
    fn omits_previous_months_when_history_not_loaded() {
        let today = NaiveDate::from_ymd_opt(2026, 3, 10).unwrap();
        let summary = analyze_usage(
            &UsageDataset {
                daily_usage: Vec::new(),
                monthly_totals: BTreeMap::new(),
                current_month_total: Some(100.0),
                current_day_total: Some(25.0),
                current_month_model_breakdown: BTreeMap::new(),
                show_model_breakdown: false,
                quota: 1_500.0,
            },
            today,
            6,
        );

        assert!(summary.previous_months.is_empty());
        assert_eq!(summary.today_used, 25.0);
    }
}
