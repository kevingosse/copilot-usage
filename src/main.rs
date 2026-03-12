mod analytics;
mod api;
mod display;
mod gh;
mod models;

use std::{env, process::ExitCode};

use analytics::analyze_usage;
use chrono::Utc;
use clap::Parser;
use colored::Colorize;
use models::UsageDataset;

#[derive(Debug, Parser)]
#[command(
    name = "copilot-usage",
    version,
    about = "Analyze GitHub Copilot premium-request usage pace."
)]
struct Cli {
    /// GitHub username for API mode
    #[arg(long)]
    username: Option<String>,

    /// Monthly quota
    #[arg(long, default_value_t = 1_500)]
    quota: u32,

    /// Number of previous months to show
    #[arg(long, default_value_t = 6)]
    months: usize,

    /// Show the full dashboard with recent daily detail and previous months
    #[arg(long)]
    full: bool,

    /// Print each gh command and its raw response to stderr
    #[arg(long)]
    debug: bool,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{} {}", "error:".red().bold(), message);
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let cli = Cli::parse();
    if cli.quota == 0 {
        return Err("--quota must be greater than zero".to_string());
    }
    if cli.months == 0 {
        return Err("--months must be greater than zero".to_string());
    }
    let today = Utc::now().date_naive();

    let username = resolve_username(cli.username.as_deref(), cli.debug)?;
    let dataset = api::load_api_usage(
        &username, cli.quota, cli.months, today, cli.full, cli.full, cli.debug,
    )?;

    render(dataset, today, cli.months);
    Ok(())
}

fn render(dataset: UsageDataset, today: chrono::NaiveDate, previous_months: usize) {
    let summary = analyze_usage(&dataset, today, previous_months);
    println!("{}", display::render_report(&summary));
}

fn resolve_username(cli_username: Option<&str>, debug: bool) -> Result<String, String> {
    if let Some(username) = cli_username
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Ok(username.to_string());
    }

    for key in ["GITHUB_USER", "GH_USERNAME"] {
        if let Ok(username) = env::var(key) {
            let username = username.trim();
            if !username.is_empty() {
                return Ok(username.to_string());
            }
        }
    }

    infer_username_from_gh(debug).ok_or_else(|| {
        "missing GitHub username (use --username, set GITHUB_USER/GH_USERNAME, or log in with gh)"
            .to_string()
    })
}

fn infer_username_from_gh(debug: bool) -> Option<String> {
    let output = crate::gh::run_gh(["api", "user", "--jq", ".login"], debug).ok()?;
    if !output.status.success() {
        return None;
    }

    let username = String::from_utf8(output.stdout).ok()?;
    let username = username.trim();
    if username.is_empty() {
        None
    } else {
        Some(username.to_string())
    }
}
