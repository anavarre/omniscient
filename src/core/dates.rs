//! Date entry and formatting.
//!
//! The web app used the browser's date picker; natively the "When?" field
//! takes free text instead, so dates can be typed without leaving the
//! keyboard: `today`, `tom`, `fri`, `next fri`, `+3`, `2w`, `in 5 days`,
//! `next week`, `oct 12`, `12 oct 2027`, `2026-10-12`, `12`.

use chrono::{Datelike, Duration, Local, Months, NaiveDate, Weekday};

pub fn today() -> NaiveDate {
    Local::now().date_naive()
}

const MONTHS: [&str; 12] = [
    "january", "february", "march", "april", "may", "june", "july", "august", "september", "october", "november",
    "december",
];
const WEEKDAYS: [(&str, Weekday); 7] = [
    ("monday", Weekday::Mon),
    ("tuesday", Weekday::Tue),
    ("wednesday", Weekday::Wed),
    ("thursday", Weekday::Thu),
    ("friday", Weekday::Fri),
    ("saturday", Weekday::Sat),
    ("sunday", Weekday::Sun),
];

fn month_of(word: &str) -> Option<u32> {
    if word.len() < 3 {
        return None;
    }
    MONTHS.iter().position(|m| m.starts_with(word)).map(|i| i as u32 + 1)
}

fn weekday_of(word: &str) -> Option<Weekday> {
    // Two letters are enough to be unambiguous ("mo", "tu", "we", "th", "fr", "sa", "su").
    if word.len() < 2 {
        return None;
    }
    WEEKDAYS.iter().find(|(name, _)| name.starts_with(word)).map(|(_, wd)| *wd)
}

/// Next occurrence of `wd` strictly after `from`.
fn next_weekday(from: NaiveDate, wd: Weekday) -> NaiveDate {
    let ahead = (wd.num_days_from_monday() as i64 - from.weekday().num_days_from_monday() as i64 + 7) % 7;
    from + Duration::days(if ahead == 0 { 7 } else { ahead })
}

/// A day/month with no year means the next time that date comes around.
fn upcoming(today: NaiveDate, month: u32, day: u32, year: Option<i32>) -> Option<NaiveDate> {
    match year {
        Some(y) => NaiveDate::from_ymd_opt(if y < 100 { 2000 + y } else { y }, month, day),
        None => {
            let this_year = NaiveDate::from_ymd_opt(today.year(), month, day)?;
            if this_year >= today { Some(this_year) } else { NaiveDate::from_ymd_opt(today.year() + 1, month, day) }
        }
    }
}

fn offset(today: NaiveDate, n: i64, unit: &str) -> Option<NaiveDate> {
    match unit {
        "" | "d" | "day" | "days" => Some(today + Duration::days(n)),
        "w" | "wk" | "wks" | "week" | "weeks" => Some(today + Duration::weeks(n)),
        "m" | "mo" | "month" | "months" => add_months(today, n),
        "y" | "yr" | "year" | "years" => add_months(today, n.checked_mul(12)?),
        _ => None,
    }
}

fn add_months(d: NaiveDate, n: i64) -> Option<NaiveDate> {
    let months = Months::new(u32::try_from(n.unsigned_abs()).ok()?);
    if n >= 0 { d.checked_add_months(months) } else { d.checked_sub_months(months) }
}

/// Splits "12oct" / "3d" / "+2w" into number and unit.
fn split_num(token: &str) -> Option<(i64, &str)> {
    let digits_end = token.find(|c: char| !c.is_ascii_digit() && c != '+' && c != '-').unwrap_or(token.len());
    let (num, unit) = token.split_at(digits_end);
    if num.is_empty() || num == "+" || num == "-" {
        return None;
    }
    num.parse::<i64>().ok().map(|n| (n, unit))
}

pub fn parse(input: &str, today: NaiveDate) -> Option<NaiveDate> {
    let text = input.trim().to_lowercase().replace([',', '.'], " ");
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return None;
    }

    if let Ok(d) = NaiveDate::parse_from_str(words[0], "%Y-%m-%d") {
        return (words.len() == 1).then_some(d);
    }

    match words.as_slice() {
        ["today" | "tod" | "now"] => return Some(today),
        ["tomorrow" | "tom" | "tmr" | "tmrw"] => return Some(today + Duration::days(1)),
        ["yesterday"] => return Some(today - Duration::days(1)),
        ["next", "week"] => return Some(next_weekday(today, Weekday::Mon)),
        ["next", "month"] => {
            let first = today.with_day(1)?;
            return first.checked_add_months(Months::new(1));
        }
        ["weekend"] | ["this", "weekend"] => return Some(next_weekday(today - Duration::days(1), Weekday::Sat)),
        ["in", n, unit] => return offset(today, n.parse().ok()?, unit),
        ["in", token] => {
            let (n, unit) = split_num(token)?;
            return offset(today, n, unit);
        }
        _ => {}
    }

    // Weekdays: "fri", "this fri", "next fri" (a week after the coming one).
    let (skip, rest) = match words[0] {
        "next" => (true, &words[1..]),
        "this" => (false, &words[1..]),
        _ => (false, &words[..]),
    };
    if rest.len() == 1
        && let Some(wd) = weekday_of(rest[0]) {
            let d = next_weekday(today, wd);
            return Some(if skip { d + Duration::days(7) } else { d });
        }

    // Relative offsets: "+3", "3d", "2w", "1m", "3 days", "-1".
    if let Some((n, unit)) = split_num(words[0]) {
        let explicit_sign = words[0].starts_with('+') || words[0].starts_with('-');
        if words.len() == 1 && !unit.is_empty() && month_of(unit).is_none() {
            return offset(today, n, unit);
        }
        if words.len() == 1 && unit.is_empty() && explicit_sign {
            return offset(today, n, "d");
        }
        if words.len() == 2 && unit.is_empty() && month_of(words[1]).is_none() {
            return offset(today, n, words[1]);
        }
    }

    // Day + month in either order, optional year: "oct 12", "12 oct 2027", "12oct".
    let mut day = None;
    let mut month = None;
    let mut year = None;
    for word in &words {
        if let Some(m) = month_of(word) {
            month = Some(m);
        } else if let Some((n, unit)) = split_num(word) {
            if !unit.is_empty() {
                month = Some(month_of(unit)?);
                day = Some(n as u32);
            } else if day.is_none() && (1..=31).contains(&n) {
                day = Some(n as u32);
            } else if year.is_none() && (n >= 1000 || (day.is_some() && month.is_some())) {
                year = Some(n as i32);
            } else {
                return None;
            }
        } else {
            return None;
        }
    }
    match (day, month) {
        (Some(d), Some(m)) => upcoming(today, m, d, year),
        // A bare day of the month: the next time that day comes around.
        (Some(d), None) if words.len() == 1 => {
            let this_month = NaiveDate::from_ymd_opt(today.year(), today.month(), d);
            match this_month {
                Some(date) if date >= today => Some(date),
                _ => {
                    let mut probe = today.with_day(1)?;
                    for _ in 0..12 {
                        probe = probe.checked_add_months(Months::new(1))?;
                        if let Some(date) = NaiveDate::from_ymd_opt(probe.year(), probe.month(), d) {
                            return Some(date);
                        }
                    }
                    None
                }
            }
        }
        _ => None,
    }
}

/// Row badge format: "Mar 5", with the year when it isn't this year.
pub fn short(d: NaiveDate, today: NaiveDate) -> String {
    if d.year() == today.year() { d.format("%b %-d").to_string() } else { d.format("%b %-d, %Y").to_string() }
}

/// Validation message format: "Mar 5, 2026".
pub fn medium(d: NaiveDate) -> String {
    d.format("%b %-d, %Y").to_string()
}

/// Archived-on format: "5 Mar 2026".
pub fn day_month_year(d: NaiveDate) -> String {
    d.format("%-d %b %Y").to_string()
}

/// Feedback next to the date field: "Today", "Tomorrow", "Fri, Oct 3 · in 2 days".
pub fn describe(d: NaiveDate, today: NaiveDate) -> String {
    let days = (d - today).num_days();
    let name = d.format("%a, %b %-d").to_string();
    let name = if d.year() == today.year() { name } else { format!("{name}, {}", d.year()) };
    match days {
        0 => format!("Today · {name}"),
        1 => format!("Tomorrow · {name}"),
        -1 => format!("Yesterday · {name}"),
        n if n > 1 => format!("{name} · in {n} days"),
        n => format!("{name} · {} days ago", -n),
    }
}

/// Distance from today for the When? header: "today", "in 3 days", "2 days ago".
pub fn relative(d: NaiveDate, today: NaiveDate) -> String {
    match (d - today).num_days() {
        0 => "today".into(),
        1 => "tomorrow".into(),
        -1 => "yesterday".into(),
        n if n > 1 => format!("in {n} days"),
        n => format!("{} days ago", -n),
    }
}

pub fn long(d: NaiveDate) -> String {
    d.format("%A, %B %-d").to_string()
}

pub fn iso(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

pub fn from_iso(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s.get(..10)?, "%Y-%m-%d").ok()
}

pub fn now_stamp() -> String {
    Local::now().naive_local().format("%Y-%m-%d %H:%M:%S").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    // Thursday, October 1 2026.
    fn t() -> NaiveDate {
        d(2026, 10, 1)
    }

    #[test]
    fn keywords() {
        assert_eq!(parse("today", t()), Some(t()));
        assert_eq!(parse(" Tom ", t()), Some(d(2026, 10, 2)));
        assert_eq!(parse("yesterday", t()), Some(d(2026, 9, 30)));
        assert_eq!(parse("next week", t()), Some(d(2026, 10, 5)));
        assert_eq!(parse("next month", t()), Some(d(2026, 11, 1)));
        assert_eq!(parse("", t()), None);
        assert_eq!(parse("whenever", t()), None);
    }

    #[test]
    fn weekdays() {
        assert_eq!(parse("fri", t()), Some(d(2026, 10, 2)));
        assert_eq!(parse("friday", t()), Some(d(2026, 10, 2)));
        assert_eq!(parse("thu", t()), Some(d(2026, 10, 8)), "same weekday means next week");
        assert_eq!(parse("wed", t()), Some(d(2026, 10, 7)));
        assert_eq!(parse("next fri", t()), Some(d(2026, 10, 9)));
        assert_eq!(parse("mo", t()), Some(d(2026, 10, 5)));
    }

    #[test]
    fn offsets() {
        assert_eq!(parse("+3", t()), Some(d(2026, 10, 4)));
        assert_eq!(parse("3d", t()), Some(d(2026, 10, 4)));
        assert_eq!(parse("2w", t()), Some(d(2026, 10, 15)));
        assert_eq!(parse("1m", t()), Some(d(2026, 11, 1)));
        assert_eq!(parse("in 5 days", t()), Some(d(2026, 10, 6)));
        assert_eq!(parse("in 2w", t()), Some(d(2026, 10, 15)));
        assert_eq!(parse("10 days", t()), Some(d(2026, 10, 11)));
        assert_eq!(parse("1y", t()), Some(d(2027, 10, 1)));
        assert_eq!(parse("-1y", t()), Some(d(2025, 10, 1)), "negative years go back");
        assert_eq!(parse("-2m", t()), Some(d(2026, 8, 1)));
    }

    #[test]
    fn calendar_dates() {
        assert_eq!(parse("2026-12-24", t()), Some(d(2026, 12, 24)));
        assert_eq!(parse("oct 12", t()), Some(d(2026, 10, 12)));
        assert_eq!(parse("12 October", t()), Some(d(2026, 10, 12)));
        assert_eq!(parse("12oct", t()), Some(d(2026, 10, 12)));
        assert_eq!(parse("sep 3", t()), Some(d(2027, 9, 3)), "past dates roll to next year");
        assert_eq!(parse("Mar 5, 2028", t()), Some(d(2028, 3, 5)));
        assert_eq!(parse("15", t()), Some(d(2026, 10, 15)));
        assert_eq!(parse("1", t()), Some(d(2026, 10, 1)));
        assert_eq!(parse("31", d(2026, 11, 2)), Some(d(2026, 12, 31)));
        assert_eq!(parse("feb 30", t()), None);
    }

    #[test]
    fn formats() {
        assert_eq!(short(d(2026, 3, 5), t()), "Mar 5");
        assert_eq!(short(d(2027, 3, 5), t()), "Mar 5, 2027");
        assert_eq!(medium(d(2026, 3, 5)), "Mar 5, 2026");
        assert_eq!(day_month_year(d(2026, 3, 5)), "5 Mar 2026");
        assert_eq!(describe(d(2026, 10, 2), t()), "Tomorrow · Fri, Oct 2");
        assert_eq!(describe(d(2026, 10, 6), t()), "Tue, Oct 6 · in 5 days");
        assert_eq!(relative(d(2026, 10, 6), t()), "in 5 days");
        assert_eq!(relative(d(2026, 10, 2), t()), "tomorrow");
        assert_eq!(long(d(2026, 10, 1)), "Thursday, October 1");
    }
}
