//! Gregorian dates and D3-style axis text without a clock, locale, or time zone.

use alloc::format;
use alloc::string::String;
use core::fmt::Write;

use super::TimeUnit;

pub const DAY_MS: i64 = 86_400_000;
const MONTH_SHORT: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const MONTH_LONG: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const DAY_SHORT: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const DAY_LONG: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DateParts {
    pub year: i32,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    pub millisecond: u16,
}

pub fn leap(year: i32) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

pub fn month_days(year: i32, month: u8) -> Option<u8> {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => Some(31),
        4 | 6 | 9 | 11 => Some(30),
        2 => Some(if leap(year) { 29 } else { 28 }),
        _ => None,
    }
}

/// Days since 1970-01-01, using the Gregorian 400-year cycle.
pub fn days_from_civil(year: i32, month: u8, day: u8) -> Option<i64> {
    if !(1..=9999).contains(&year) || day == 0 || day > month_days(year, month)? {
        return None;
    }
    let y = i64::from(year) - i64::from(month <= 2);
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let m = i64::from(month) + if month > 2 { -3 } else { 9 };
    let doy = (153 * m + 2) / 5 + i64::from(day) - 1;
    Some(era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468)
}

pub fn from_parts(p: DateParts) -> Option<i64> {
    if p.hour > 23 || p.minute > 59 || p.second > 59 || p.millisecond > 999 {
        return None;
    }
    days_from_civil(p.year, p.month, p.day)?
        .checked_mul(DAY_MS)?
        .checked_add(
            i64::from(p.hour) * 3_600_000
                + i64::from(p.minute) * 60_000
                + i64::from(p.second) * 1_000
                + i64::from(p.millisecond),
        )
}

pub fn date_parts(ms: i64) -> DateParts {
    let days = ms.div_euclid(DAY_MS);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    let time = ms.rem_euclid(DAY_MS);
    DateParts {
        year: year.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
        month: month as u8,
        day: day as u8,
        hour: (time / 3_600_000) as u8,
        minute: (time / 60_000 % 60) as u8,
        second: (time / 1_000 % 60) as u8,
        millisecond: (time % 1_000) as u16,
    }
}

pub fn weekday(ms: i64) -> u8 {
    (ms.div_euclid(DAY_MS) + 4).rem_euclid(7) as u8
}

pub fn format_iso(ms: i64) -> String {
    let p = date_parts(ms);
    if ms.rem_euclid(DAY_MS) == 0 {
        format!("{:04}-{:02}-{:02}", p.year, p.month, p.day)
    } else {
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
            p.year, p.month, p.day, p.hour, p.minute, p.second, p.millisecond
        )
    }
}

/// D3 time-format directives used by Mermaid, with UTC and English names.
pub fn format_axis(ms: i64, fmt: &str) -> Option<String> {
    let p = date_parts(ms);
    let week_day = usize::from(weekday(ms));
    let year_start = days_from_civil(p.year, 1, 1)?;
    let ordinal = ms.div_euclid(DAY_MS) - year_start;
    let mut out = String::new();
    let mut chars = fmt.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        let mut modifier = '0';
        let mut code = chars.next()?;
        if matches!(code, '-' | '_' | '0') {
            modifier = code;
            code = chars.next()?;
        }
        let number = match code {
            '%' => {
                out.push('%');
                continue;
            }
            'a' => {
                out.push_str(DAY_SHORT[week_day]);
                continue;
            }
            'A' => {
                out.push_str(DAY_LONG[week_day]);
                continue;
            }
            'b' | 'h' => {
                out.push_str(MONTH_SHORT[usize::from(p.month - 1)]);
                continue;
            }
            'B' => {
                out.push_str(MONTH_LONG[usize::from(p.month - 1)]);
                continue;
            }
            'c' => {
                out.push_str(&format_axis(ms, "%a %b %e %H:%M:%S %Y")?);
                continue;
            }
            'x' => {
                out.push_str(&format_axis(ms, "%m/%d/%Y")?);
                continue;
            }
            'X' => {
                out.push_str(&format_axis(ms, "%H:%M:%S")?);
                continue;
            }
            'p' => {
                out.push_str(if p.hour < 12 { "AM" } else { "PM" });
                continue;
            }
            'Z' => {
                out.push_str("+0000");
                continue;
            }
            'd' => (i64::from(p.day), 2),
            'e' => {
                if modifier == '0' {
                    modifier = '_';
                }
                (i64::from(p.day), 2)
            }
            'm' => (i64::from(p.month), 2),
            'Y' => (i64::from(p.year), 4),
            'y' => (i64::from(p.year % 100), 2),
            'H' => (i64::from(p.hour), 2),
            'I' => (i64::from((p.hour + 11) % 12 + 1), 2),
            'M' => (i64::from(p.minute), 2),
            'S' => (i64::from(p.second), 2),
            'L' => (i64::from(p.millisecond), 3),
            'j' => (ordinal + 1, 3),
            'w' => (week_day as i64, 1),
            'u' => (if week_day == 0 { 7 } else { week_day as i64 }, 1),
            'U' => ((ordinal + 7 - week_day as i64) / 7, 2),
            'W' => ((ordinal + 7 - ((week_day + 6) % 7) as i64) / 7, 2),
            'Q' => (ms, 1),
            's' => (ms.div_euclid(1_000), 1),
            _ => return None,
        };
        match modifier {
            '-' => write!(out, "{}", number.0).ok()?,
            '_' => write!(out, "{:width$}", number.0, width = number.1).ok()?,
            _ => write!(out, "{:0width$}", number.0, width = number.1).ok()?,
        }
    }
    Some(out)
}

/// Adds fixed units, or whole calendar months/years with day clamping.
pub fn add(ms: i64, amount: i64, unit: TimeUnit) -> Option<i64> {
    let scale = match unit {
        TimeUnit::Millisecond => 1,
        TimeUnit::Second => 1_000,
        TimeUnit::Minute => 60_000,
        TimeUnit::Hour => 3_600_000,
        TimeUnit::Day => DAY_MS,
        TimeUnit::Week => 7 * DAY_MS,
        TimeUnit::Month | TimeUnit::Year => {
            let mut p = date_parts(ms);
            let delta = amount.checked_mul(if unit == TimeUnit::Year { 12 } else { 1 })?;
            let months = i64::from(p.year)
                .checked_mul(12)?
                .checked_add(i64::from(p.month) - 1)?
                .checked_add(delta)?;
            p.year = i32::try_from(months.div_euclid(12)).ok()?;
            p.month = (months.rem_euclid(12) + 1) as u8;
            p.day = p.day.min(month_days(p.year, p.month)?);
            return from_parts(p);
        }
    };
    let result = ms.checked_add(amount.checked_mul(scale)?)?;
    let p = date_parts(result);
    (1..=9999).contains(&p.year).then_some(result)
}

fn digits(input: &str, at: &mut usize, min: usize, max: usize) -> Option<i64> {
    let start = *at;
    let bytes = input.as_bytes();
    while *at < bytes.len() && *at - start < max && bytes[*at].is_ascii_digit() {
        *at += 1;
    }
    if *at - start < min {
        return None;
    }
    input.get(start..*at)?.parse().ok()
}

/// Strict dateFormat parsing. Omitted components use 1970-01-01 rather than the clock.
pub fn parse_date(input: &str, fmt: &str) -> Option<i64> {
    if fmt == "x" {
        return input
            .parse::<i64>()
            .ok()
            .filter(|v| (1..=9999).contains(&date_parts(*v).year));
    }
    if fmt == "X" {
        return decimal_scaled(input, 1_000).filter(|v| (1..=9999).contains(&date_parts(*v).year));
    }
    let mut p = DateParts {
        year: 1970,
        month: 1,
        day: 1,
        hour: 0,
        minute: 0,
        second: 0,
        millisecond: 0,
    };
    let (mut at, mut fi) = (0usize, 0usize);
    let mut offset = 0i64;
    let mut ordinal = None;
    let mut temporal = false;
    let mut hour12 = false;
    let mut ampm = None;
    let tokens = [
        "YYYY", "MMMM", "DDDD", "MMM", "DDD", "SSS", "YY", "MM", "DD", "Do", "HH", "hh", "mm",
        "ss", "SS", "ZZ", "Q", "M", "D", "H", "h", "m", "s", "S", "Z", "A", "a",
    ];
    while fi < fmt.len() {
        let rest = fmt.get(fi..)?;
        if rest.starts_with('[') {
            let end = rest.find(']')?;
            let literal = rest.get(1..end)?;
            if !input.get(at..)?.starts_with(literal) {
                return None;
            }
            at += literal.len();
            fi += end + 1;
            continue;
        }
        let token = tokens.iter().find(|t| rest.starts_with(**t)).copied();
        let Some(token) = token else {
            let c = rest.chars().next()?;
            if input.get(at..)?.chars().next()? != c {
                return None;
            }
            at += c.len_utf8();
            fi += c.len_utf8();
            continue;
        };
        fi += token.len();
        temporal = true;
        match token {
            "MMM" | "MMMM" => {
                let names = if token == "MMM" {
                    &MONTH_SHORT
                } else {
                    &MONTH_LONG
                };
                let (i, name) = names.iter().enumerate().find(|(_, n)| {
                    input.get(at..).is_some_and(|r| {
                        r.get(..n.len()).is_some_and(|v| v.eq_ignore_ascii_case(n))
                    })
                })?;
                p.month = (i + 1) as u8;
                at += name.len();
            }
            "A" | "a" => {
                let v = input.get(at..at + 2)?;
                ampm = Some(if v.eq_ignore_ascii_case("am") {
                    false
                } else if v.eq_ignore_ascii_case("pm") {
                    true
                } else {
                    return None;
                });
                at += 2;
            }
            "Z" | "ZZ" => {
                if input.get(at..)?.starts_with('Z') {
                    at += 1;
                    continue;
                }
                let sign = match input.as_bytes().get(at)? {
                    b'+' => 1,
                    b'-' => -1,
                    _ => return None,
                };
                at += 1;
                let h = digits(input, &mut at, 2, 2)?;
                if token == "Z" && input.get(at..)?.starts_with(':') {
                    at += 1;
                }
                let m = digits(input, &mut at, 2, 2)?;
                if h > 23 || m > 59 {
                    return None;
                }
                offset = sign * (h * 60 + m) * 60_000;
            }
            _ => {
                let (min, max) = match token {
                    "YYYY" => (4, 4),
                    "YY" | "MM" | "DD" | "HH" | "hh" | "mm" | "ss" | "SS" => (2, 2),
                    "SSS" | "DDDD" => (3, 3),
                    "DDD" => (1, 3),
                    "Q" | "S" => (1, 1),
                    _ => (1, 2),
                };
                let n = digits(input, &mut at, min, max)?;
                match token {
                    "YYYY" => {
                        p.year = n as i32;
                    }
                    "YY" => {
                        p.year = if n > 68 {
                            1900 + n as i32
                        } else {
                            2000 + n as i32
                        };
                    }
                    "M" | "MM" => {
                        p.month = n as u8;
                    }
                    "D" | "DD" | "Do" => {
                        p.day = n as u8;
                        if token == "Do" {
                            let suffix = input.get(at..at + 2)?;
                            if !["st", "nd", "rd", "th"].contains(&suffix) {
                                return None;
                            }
                            at += 2;
                        }
                    }
                    "Q" => {
                        if !(1..=4).contains(&n) {
                            return None;
                        }
                        p.month = ((n - 1) * 3 + 1) as u8;
                    }
                    "DDD" | "DDDD" => {
                        ordinal = Some(n);
                    }
                    "H" | "HH" => p.hour = n as u8,
                    "h" | "hh" => {
                        if !(1..=12).contains(&n) {
                            return None;
                        }
                        p.hour = (n % 12) as u8;
                        hour12 = true;
                    }
                    "m" | "mm" => p.minute = n as u8,
                    "s" | "ss" => p.second = n as u8,
                    "S" => p.millisecond = (n * 100) as u16,
                    "SS" => p.millisecond = (n * 10) as u16,
                    "SSS" => p.millisecond = n as u16,
                    _ => return None,
                }
            }
        }
    }
    if at != input.len() {
        return None;
    }
    if !temporal {
        return None;
    }
    if let Some(n) = ordinal {
        if n < 1 || n > if leap(p.year) { 366 } else { 365 } {
            return None;
        }
        let date = date_parts(
            days_from_civil(p.year, 1, 1)?
                .checked_add(n - 1)?
                .checked_mul(DAY_MS)?,
        );
        p.month = date.month;
        p.day = date.day;
    }
    if hour12 {
        p.hour += if ampm? { 12 } else { 0 };
    }
    from_parts(p)?
        .checked_sub(offset)
        .filter(|v| (1..=9999).contains(&date_parts(*v).year))
}

/// Decimal to integer scale, truncated like a JavaScript Date timestamp.
pub fn decimal_scaled(value: &str, scale: i64) -> Option<i64> {
    let (negative, body) = match value.strip_prefix('-') {
        Some(v) => (true, v),
        None => (false, value.strip_prefix('+').unwrap_or(value)),
    };
    let (whole, frac) = body.split_once('.').unwrap_or((body, ""));
    if whole.is_empty()
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || !frac.bytes().all(|b| b.is_ascii_digit())
        || frac.len() > 9
    {
        return None;
    }
    let whole: i64 = whole.parse().ok()?;
    let denominator = 10i64.checked_pow(frac.len() as u32)?;
    let fraction = if frac.is_empty() {
        0
    } else {
        frac.parse::<i64>().ok()?
    };
    let n = whole
        .checked_mul(scale)?
        .checked_add(fraction.checked_mul(scale)?.checked_div(denominator)?)?;
    Some(if negative { -n } else { n })
}

pub fn duration(value: &str) -> Option<(i64, TimeUnit)> {
    let units = [
        ("ms", TimeUnit::Millisecond),
        ("s", TimeUnit::Second),
        ("m", TimeUnit::Minute),
        ("h", TimeUnit::Hour),
        ("d", TimeUnit::Day),
        ("w", TimeUnit::Week),
        ("M", TimeUnit::Month),
        ("y", TimeUnit::Year),
    ];
    for (suffix, unit) in units {
        if let Some(number) = value.strip_suffix(suffix) {
            if number.is_empty()
                || !number.as_bytes().first().is_some_and(u8::is_ascii_digit)
                || number.ends_with('.')
            {
                return None;
            }
            if matches!(unit, TimeUnit::Month | TimeUnit::Year) {
                decimal_scaled(number, 1)?;
                return Some((number.split('.').next()?.parse().ok()?, unit));
            }
            if matches!(unit, TimeUnit::Day | TimeUnit::Week) {
                let doubled = decimal_scaled(number, if unit == TimeUnit::Week { 14 } else { 2 })?;
                return Some((doubled.checked_add(1)? / 2, TimeUnit::Day));
            }
            let scale = match unit {
                TimeUnit::Millisecond => 1,
                TimeUnit::Second => 1_000,
                TimeUnit::Minute => 60_000,
                TimeUnit::Hour => 3_600_000,
                _ => return None,
            };
            let amount = decimal_scaled(number, scale)?;
            if amount < 0 {
                return None;
            }
            return Some((
                amount,
                if matches!(unit, TimeUnit::Month | TimeUnit::Year) {
                    unit
                } else {
                    TimeUnit::Millisecond
                },
            ));
        }
    }
    None
}

pub fn time_only_format(fmt: &str) -> bool {
    let parts = date_components(fmt);
    !parts.0 && !parts.1 && !parts.2
}

pub fn uses_epoch_defaults(fmt: &str) -> bool {
    let parts = date_components(fmt);
    !(parts.0 && parts.1 && parts.2)
}

fn date_components(fmt: &str) -> (bool, bool, bool) {
    if matches!(fmt, "x" | "X") {
        return (true, true, true);
    }
    let (mut year, mut month, mut day) = (false, false, false);
    let mut at = 0;
    while at < fmt.len() {
        let Some(rest) = fmt.get(at..) else {
            break;
        };
        if rest.starts_with('[') {
            let Some(end) = rest.find(']') else {
                break;
            };
            at += end + 1;
            continue;
        }
        if rest.starts_with("DDD") {
            month = true;
            day = true;
        }
        let Some(c) = rest.chars().next() else {
            break;
        };
        match c {
            'Y' => year = true,
            'M' | 'Q' => month = true,
            'D' => day = true,
            _ => {}
        }
        at += c.len_utf8();
    }
    (year, month, day)
}

/// Validates supported Dayjs tokens independently of the task values.
pub fn valid_date_format(fmt: &str) -> bool {
    if matches!(fmt, "x" | "X") {
        return true;
    }
    let tokens = [
        "YYYY", "MMMM", "DDDD", "MMM", "DDD", "SSS", "YY", "MM", "DD", "Do", "HH", "hh", "mm",
        "ss", "SS", "ZZ", "Q", "M", "D", "H", "h", "m", "s", "S", "Z", "A", "a",
    ];
    let mut at = 0;
    let mut temporal = false;
    while at < fmt.len() {
        let Some(rest) = fmt.get(at..) else {
            return false;
        };
        if rest.starts_with('[') {
            let Some(end) = rest.find(']') else {
                return false;
            };
            at += end + 1;
            continue;
        }
        if let Some(token) = tokens.iter().find(|token| rest.starts_with(**token)) {
            at += token.len();
            temporal = true;
            continue;
        }
        let Some(c) = rest.chars().next() else {
            return false;
        };
        if c.is_ascii_alphabetic() && c != 'T' {
            return false;
        }
        at += c.len_utf8();
    }
    temporal
}

pub fn weekday_name(value: &str) -> Option<u8> {
    DAY_LONG
        .iter()
        .position(|d| d.eq_ignore_ascii_case(value))
        .map(|i| i as u8)
}

pub fn tick_interval(value: &str) -> Option<super::TickInterval> {
    let n = value.find(|c: char| !c.is_ascii_digit())?;
    let count = value.get(..n)?.parse::<u32>().ok()?;
    if count == 0 {
        return None;
    }
    let unit = match value.get(n..)? {
        "millisecond" => TimeUnit::Millisecond,
        "second" => TimeUnit::Second,
        "minute" => TimeUnit::Minute,
        "hour" => TimeUnit::Hour,
        "day" => TimeUnit::Day,
        "week" => TimeUnit::Week,
        "month" => TimeUnit::Month,
        "year" => TimeUnit::Year,
        _ => return None,
    };
    Some(super::TickInterval { count, unit })
}
