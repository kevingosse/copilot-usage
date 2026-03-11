use std::collections::BTreeMap;

use chrono::NaiveDate;

#[derive(Clone, Debug)]
pub struct DailyUsage {
    pub date: NaiveDate,
    pub total_requests: f64,
    pub model_breakdown: BTreeMap<String, f64>,
    pub total_monthly_quota: Option<f64>,
    pub exceeds_quota: bool,
}

impl DailyUsage {
    pub fn new(date: NaiveDate) -> Self {
        Self {
            date,
            total_requests: 0.0,
            model_breakdown: BTreeMap::new(),
            total_monthly_quota: None,
            exceeds_quota: false,
        }
    }

    pub fn add_quantity(
        &mut self,
        quantity: f64,
        model: Option<&str>,
        total_monthly_quota: Option<f64>,
        exceeds_quota: bool,
    ) {
        self.total_requests += quantity;
        if let Some(model) = model.map(str::trim).filter(|value| !value.is_empty()) {
            *self.model_breakdown.entry(model.to_string()).or_insert(0.0) += quantity;
        }
        if total_monthly_quota.is_some() {
            self.total_monthly_quota = total_monthly_quota;
        }
        self.exceeds_quota |= exceeds_quota;
    }
}

#[derive(Clone, Debug)]
pub struct UsageDataset {
    pub daily_usage: Vec<DailyUsage>,
    pub monthly_totals: BTreeMap<NaiveDate, f64>,
    pub current_month_total: Option<f64>,
    pub current_day_total: Option<f64>,
    pub current_month_model_breakdown: BTreeMap<String, f64>,
    pub show_model_breakdown: bool,
    pub quota: f64,
}

#[derive(Clone, Debug)]
pub struct MonthlyUsage {
    pub month_start: NaiveDate,
    pub total_requests: f64,
}

#[derive(Clone, Debug)]
pub struct UsageSummary {
    pub month_start: NaiveDate,
    pub today: NaiveDate,
    pub quota: f64,
    pub mtd_used: f64,
    pub remaining_quota: f64,
    pub percent_consumed: f64,
    pub days_elapsed: u32,
    pub days_remaining: u32,
    pub avg_daily_usage: f64,
    pub today_used: f64,
    pub projected_over_quota: bool,
    pub cushion_or_overshoot: f64,
    pub projected_run_out_date: Option<NaiveDate>,
    pub daily_budget: Option<f64>,
    pub recent_days: Vec<(NaiveDate, f64)>,
    pub previous_months: Vec<MonthlyUsage>,
    pub model_breakdown: Vec<(String, f64)>,
    pub model_breakdown_available: bool,
    pub show_model_breakdown: bool,
}
