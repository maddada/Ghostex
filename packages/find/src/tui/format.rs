use crate::index::{day_key, SECONDS_PER_DAY, UNKNOWN_DAY_KEY};
use crate::scan::civil_from_day_key;

pub fn now_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn month_name(month: u32) -> &'static str {
    match month {
        1 => "Jan",
        2 => "Feb",
        3 => "Mar",
        4 => "Apr",
        5 => "May",
        6 => "Jun",
        7 => "Jul",
        8 => "Aug",
        9 => "Sep",
        10 => "Oct",
        11 => "Nov",
        12 => "Dec",
        _ => "???",
    }
}

pub fn format_last_active_compact(ts: i64, now: i64) -> String {
    if ts <= 0 {
        return "unknown".to_string();
    }
    let delta = (now - ts).max(0);
    if delta < 60 {
        return "now".to_string();
    }
    if delta < 3_600 {
        return format!("{}m ago", delta / 60);
    }
    if delta < SECONDS_PER_DAY {
        return format!("{}h ago", delta / 3_600);
    }
    if delta < 7 * SECONDS_PER_DAY {
        return format!("{}d ago", delta / SECONDS_PER_DAY);
    }
    let date = civil_from_day_key(day_key(ts));
    format!("{} {}", month_name(date.month), date.day)
}

pub fn format_day_header(day: i64, now: i64) -> String {
    if day == UNKNOWN_DAY_KEY {
        return "Unknown day".to_string();
    }
    let today = day_key(now);
    if day == today {
        return "Today".to_string();
    }
    if day == today - 1 {
        return "Yesterday".to_string();
    }
    if day > today - 7 && day < today {
        return format!("{} days ago", today - day);
    }
    let date = civil_from_day_key(day);
    let now_date = civil_from_day_key(today);
    if date.year == now_date.year {
        return format!("{} {}", month_name(date.month), date.day);
    }
    format!("{} {}, {}", month_name(date.month), date.day, date.year)
}

pub fn format_last_active_full(ts: i64) -> String {
    if ts <= 0 {
        return "last active unknown".to_string();
    }
    let date = civil_from_day_key(day_key(ts));
    let seconds = ts.rem_euclid(SECONDS_PER_DAY);
    let hour = seconds / 3_600;
    let minute = (seconds % 3_600) / 60;
    format!(
        "last active {} {} {:02}:{:02} UTC",
        month_name(date.month),
        date.day,
        hour,
        minute
    )
}
