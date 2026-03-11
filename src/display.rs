use colored::{ColoredString, Colorize};
use unicode_width::UnicodeWidthStr;

use crate::models::{MonthlyUsage, UsageSummary};

pub fn render_report(summary: &UsageSummary) -> String {
    let mut sections = Vec::new();
    sections.push(render_header(summary));
    sections.push(render_pace_projection(summary));
    if !summary.recent_days.is_empty() {
        sections.push(render_recent_sparkline(summary));
    }
    if !summary.previous_months.is_empty() {
        sections.push(render_previous_months(summary));
    }
    if summary.show_model_breakdown {
        if summary.model_breakdown_available {
            sections.push(render_model_breakdown(summary));
        } else {
            sections.push(render_model_breakdown_unavailable());
        }
    }

    sections.join("\n\n")
}

fn render_header(summary: &UsageSummary) -> String {
    let month_label = summary.month_start.format("%B %Y").to_string();
    let percent_text = format!("{:.1}%", summary.percent_consumed);
    let content = vec![
        "🤖 GitHub Copilot Premium Request Usage".to_string(),
        format!(
            "Month: {month_label}   Quota: {}",
            format_quantity(summary.quota)
        ),
        format!("As of: {} UTC", summary.today.format("%B %d, %Y")),
        format!(
            "Used: {}  Remaining: {}  ({} consumed)",
            format_quantity(summary.mtd_used),
            format_quantity(summary.remaining_quota),
            percent_text
        ),
        format!(
            "{}  {}",
            plain_progress_bar(
                (summary.percent_consumed / 100.0).clamp(0.0, 1.0),
                26,
                summary.projected_over_quota,
            ),
            percent_text
        ),
    ];

    render_box(
        &content,
        if summary.projected_over_quota {
            "red"
        } else {
            "cyan"
        },
    )
}

fn render_pace_projection(summary: &UsageSummary) -> String {
    let mut lines = vec![
        format!("{}", "📈 Pace & Projection".bold().bright_cyan()),
        divider(),
        metric_line(
            "Avg daily usage (MTD):",
            &format!("{:.1} req/day", summary.avg_daily_usage),
        ),
        metric_line(
            "Days elapsed:",
            &format!(
                "{} / {}",
                summary.days_elapsed,
                summary.days_elapsed + summary.days_remaining
            ),
        ),
        metric_line("Today budget:", &format_today_budget_bar(summary)),
    ];

    let projected_line = format_projected_outcome(summary);
    lines.push(metric_line("Projected month-end:", &projected_line));

    if summary.projected_over_quota {
        let run_out_text = summary
            .projected_run_out_date
            .map(|date| date.format("%B %d, %Y").to_string())
            .unwrap_or_else(|| "Unavailable".to_string());
        lines.push(metric_line("Estimated run-out date:", &run_out_text));
    }

    lines.join("\n")
}

fn render_recent_sparkline(summary: &UsageSummary) -> String {
    let labels = summary
        .recent_days
        .iter()
        .map(|(date, _)| date.format("%b %d").to_string())
        .collect::<Vec<_>>()
        .join("  ");
    let points = summary
        .recent_days
        .iter()
        .map(|(_, value)| *value)
        .collect::<Vec<_>>();

    [
        format!("{}", "📊 Daily Usage (last 14 days)".bold().bright_cyan()),
        divider(),
        labels.dimmed().to_string(),
        sparkline(&points).bright_green().bold().to_string(),
    ]
    .join("\n")
}

fn render_previous_months(summary: &UsageSummary) -> String {
    let mut lines = vec![
        format!("{}", "📅 Previous Months".bold().bright_cyan()),
        divider(),
    ];

    for month in &summary.previous_months {
        lines.push(render_month_bar(month, summary.quota));
    }

    lines.join("\n")
}

fn render_model_breakdown(summary: &UsageSummary) -> String {
    let mut lines = vec![
        format!(
            "{}",
            "🔧 Model Breakdown (Current Month)".bold().bright_cyan()
        ),
        divider(),
    ];
    let total = summary.mtd_used.max(1.0);
    for (model, quantity) in &summary.model_breakdown {
        let ratio = *quantity / total;
        let bar = horizontal_bar(ratio, 16, false);
        let model_label = format!("{:<18}", truncate(model, 18));
        lines.push(format!(
            "{} │ {}  {:>6} req  ({:>5.1}%)",
            model_label,
            bar,
            format_quantity(*quantity),
            ratio * 100.0
        ));
    }

    lines.join("\n")
}

fn render_model_breakdown_unavailable() -> String {
    [
        format!(
            "{}",
            "🔧 Model Breakdown (Current Month)".bold().bright_cyan()
        ),
        divider(),
        "Model-level data is unavailable from the current source."
            .dimmed()
            .to_string(),
    ]
    .join("\n")
}

fn render_month_bar(month: &MonthlyUsage, quota: f64) -> String {
    let ratio = if quota == 0.0 {
        0.0
    } else {
        month.total_requests / quota
    };
    let percentage = ratio * 100.0;
    let bar = horizontal_bar(ratio.min(1.0), 24, month.total_requests > quota);
    format!(
        "{}  {}  {:>5} / {:>5}  {:>5.1}%",
        month.month_start.format("%b %Y"),
        bar,
        format_quantity(month.total_requests),
        format_quantity(quota),
        percentage
    )
}

fn render_box(lines: &[String], accent: &str) -> String {
    let inner_width = lines
        .iter()
        .map(|line| UnicodeWidthStr::width(line.as_str()))
        .max()
        .unwrap_or(0)
        + 2;

    let mut rendered = Vec::new();
    let horizontal = "─".repeat(inner_width + 2);
    rendered.push(color_line(format!("┌{horizontal}┐"), accent));

    for line in lines {
        let padding = inner_width.saturating_sub(UnicodeWidthStr::width(line.as_str()));
        rendered.push(color_line(
            format!("│ {line}{} │", " ".repeat(padding)),
            accent,
        ));
    }

    rendered.push(color_line(format!("└{horizontal}┘"), accent));
    rendered.join("\n")
}

fn color_line(line: String, accent: &str) -> String {
    match accent {
        "red" => line.bright_red().to_string(),
        "cyan" => line.bright_cyan().to_string(),
        _ => line.to_string(),
    }
}

fn metric_line(label: &str, value: &str) -> String {
    format!("{label:<27} {value}")
}

fn divider() -> String {
    "─────────────────────────────────────────────"
        .bright_black()
        .to_string()
}

fn format_today_budget_bar(summary: &UsageSummary) -> String {
    match summary.daily_budget {
        Some(budget) if budget > 0.0 => {
            let over_budget = summary.today_used > budget;
            let ratio = (summary.today_used / budget).clamp(0.0, 1.0);
            format!(
                "{}  {} / {}",
                horizontal_bar(ratio, 16, over_budget),
                format_quantity(summary.today_used),
                format_quantity(budget)
            )
        }
        Some(_) if summary.remaining_quota <= 0.0 => format!(
            "{}  {} / 0",
            horizontal_bar(1.0, 16, true),
            format_quantity(summary.today_used)
        ),
        Some(_) => format!(
            "{}  {} / 0",
            horizontal_bar(1.0, 16, false),
            format_quantity(summary.today_used)
        ),
        None if summary.remaining_quota <= 0.0 => format!(
            "{}  {} / 0",
            horizontal_bar(1.0, 16, true),
            format_quantity(summary.today_used)
        ),
        None => "N/A".to_string(),
    }
}

fn format_projected_outcome(summary: &UsageSummary) -> String {
    let outcome = if summary.projected_over_quota {
        format!("-{} requests (OVER QUOTA ✗)", format_quantity(summary.cushion_or_overshoot.abs()))
            .red()
    } else {
        format!("+{} requests (UNDER QUOTA ✓)", format_quantity(summary.cushion_or_overshoot.max(0.0)))
            .green()
    };

    outcome.bold().to_string()
}

fn plain_progress_bar(ratio: f64, width: usize, danger: bool) -> String {
    let filled = (ratio.clamp(0.0, 1.0) * width as f64).round() as usize;
    let full = if danger { '■' } else { '█' };
    format!(
        "{}{}",
        full.to_string().repeat(filled),
        "░".repeat(width.saturating_sub(filled))
    )
}

fn horizontal_bar(ratio: f64, width: usize, danger: bool) -> ColoredString {
    let filled = (ratio.clamp(0.0, 1.0) * width as f64).round() as usize;
    let bar = format!(
        "{}{}",
        "█".repeat(filled),
        "░".repeat(width.saturating_sub(filled))
    );
    if danger {
        bar.red()
    } else if ratio >= 0.8 {
        bar.yellow()
    } else {
        bar.green()
    }
}

fn sparkline(values: &[f64]) -> String {
    const BLOCKS: [&str; 8] = ["▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];
    let max_value = values.iter().copied().fold(0.0_f64, f64::max);
    if max_value <= 0.0 {
        return "▁".repeat(values.len());
    }

    values
        .iter()
        .map(|value| {
            let index = ((*value / max_value) * (BLOCKS.len() as f64 - 1.0)).round() as usize;
            BLOCKS[index]
        })
        .collect::<String>()
}

fn format_quantity(value: f64) -> String {
    let rounded = (value * 100.0).round() / 100.0;
    if (rounded - rounded.round()).abs() < 0.005 {
        return format_integer(rounded.round() as i64);
    }

    let sign = if rounded < 0.0 { "-" } else { "" };
    let absolute = rounded.abs();
    let raw = format!("{absolute:.2}");
    let raw = raw.trim_end_matches('0').trim_end_matches('.');
    let (whole, fraction) = raw.split_once('.').unwrap_or((raw, ""));
    let formatted_whole = format_integer(whole.parse::<i64>().unwrap_or(0));
    if fraction.is_empty() {
        format!("{sign}{formatted_whole}")
    } else {
        format!("{sign}{formatted_whole}.{fraction}")
    }
}

fn format_integer(value: i64) -> String {
    let raw = value.to_string();
    let (sign, digits) = if let Some(stripped) = raw.strip_prefix('-') {
        ("-", stripped)
    } else {
        ("", raw.as_str())
    };

    let mut reversed = String::new();
    for (index, character) in digits.chars().rev().enumerate() {
        if index != 0 && index % 3 == 0 {
            reversed.push(',');
        }
        reversed.push(character);
    }

    let formatted = reversed.chars().rev().collect::<String>();
    format!("{sign}{formatted}")
}

fn truncate(value: &str, max_width: usize) -> String {
    if UnicodeWidthStr::width(value) <= max_width {
        return value.to_string();
    }

    let mut output = String::new();
    for character in value.chars() {
        let next = format!("{output}{character}");
        if UnicodeWidthStr::width(next.as_str()) >= max_width.saturating_sub(1) {
            break;
        }
        output.push(character);
    }
    output.push('…');
    output
}

#[cfg(test)]
mod tests {
    use super::{format_integer, format_quantity, sparkline};

    #[test]
    fn formats_numbers_with_commas() {
        assert_eq!(format_integer(1500), "1,500");
        assert_eq!(format_integer(42), "42");
    }

    #[test]
    fn formats_fractional_quantities() {
        assert_eq!(format_quantity(112.33), "112.33");
        assert_eq!(format_quantity(1500.0), "1,500");
    }

    #[test]
    fn renders_sparkline() {
        assert_eq!(sparkline(&[0.0, 5.0, 10.0]).chars().count(), 3);
    }
}
