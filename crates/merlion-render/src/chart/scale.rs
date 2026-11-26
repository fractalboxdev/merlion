//! Scales, ticks and tick labels (specs/chart.md#axes).
//!
//! Linear ticks use the 1-2-5 progression of Heckbert's "Nice numbers for graph labels"
//! (Graphics Gems, 1990), implemented from its description. A tick is an integer
//! multiple of `mantissa × 10^exponent`, so its label is printed from integers and never
//! from a binary fraction: 0.1 + 0.2 never prints as 0.30000000000000004.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;

use super::spec::NumberFormat;
use crate::math::{abs, ceil, floor};
use crate::model::gantt::{calendar, TickInterval, TimeUnit};

/// Exact powers of ten up to 10^22, the largest exactly representable in `f64`.
const POW10: [f64; 23] = [
    1e0, 1e1, 1e2, 1e3, 1e4, 1e5, 1e6, 1e7, 1e8, 1e9, 1e10, 1e11, 1e12, 1e13, 1e14, 1e15, 1e16,
    1e17, 1e18, 1e19, 1e20, 1e21, 1e22,
];

/// `m × 10^e` as an `f64`, correctly rounded: a negative exponent divides by an exact
/// power instead of multiplying by an inexact one.
pub fn scaled(m: i64, e: i32) -> f64 {
    let p = POW10[e.unsigned_abs().min(22) as usize];
    if e >= 0 {
        m as f64 * p
    } else {
        m as f64 / p
    }
}

/// A tick step `mantissa × 10^exponent`, mantissa 1, 2 or 5.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Step {
    pub mantissa: i64,
    pub exponent: i32,
}

impl Step {
    pub fn value(self) -> f64 {
        scaled(self.mantissa, self.exponent)
    }

    /// `v / step`, computed as `v × 10^-e / m` so exact decimals stay exact.
    fn units(self, v: f64) -> f64 {
        let p = POW10[self.exponent.unsigned_abs().min(22) as usize];
        if self.exponent >= 0 {
            v / p / self.mantissa as f64
        } else {
            v * p / self.mantissa as f64
        }
    }
}

/// The nice step for `span` divided into about `count` intervals.
pub fn nice_step(span: f64, count: f64) -> Step {
    let raw = span / count.max(1.0);
    if !(raw.is_finite() && raw > 0.0) {
        return Step {
            mantissa: 1,
            exponent: 0,
        };
    }
    let mut e: i32 = 0;
    while e > -15 && scaled(1, e) > raw {
        e -= 1;
    }
    while e < 21 && scaled(1, e + 1) <= raw {
        e += 1;
    }
    let f = raw / scaled(1, e);
    // Thresholds are the geometric means √2, √10 and √50 between the candidate mantissas.
    let (m, e) = if f >= 7.071_067_811_865_475 {
        (1, e + 1)
    } else if f >= 3.162_277_660_168_379_5 {
        (5, e)
    } else if f >= core::f64::consts::SQRT_2 {
        (2, e)
    } else {
        (1, e)
    };
    Step {
        mantissa: m,
        exponent: e,
    }
}

/// Tolerance in step units for values that are a tick but miss it by rounding.
const SNAP: f64 = 1e-9;

/// The domain widened to whole steps.
pub fn nice_domain(lo: f64, hi: f64, step: Step) -> (f64, f64) {
    let a = floor(step.units(lo) + SNAP);
    let b = ceil(step.units(hi) - SNAP);
    (
        scaled(a as i64 * step.mantissa, step.exponent),
        scaled(b as i64 * step.mantissa, step.exponent),
    )
}

/// A tick: its value and the integer `k` with value = `k × mantissa × 10^exponent`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tick {
    pub value: f64,
    pub k: i64,
}

/// Ticks inside `[lo, hi]`, at most `max` of them.
pub fn ticks(lo: f64, hi: f64, step: Step, max: usize) -> Vec<Tick> {
    let a = ceil(step.units(lo) - SNAP) as i64;
    let b = floor(step.units(hi) + SNAP) as i64;
    let mut out = Vec::new();
    let mut k = a;
    while k <= b && out.len() < max {
        out.push(Tick {
            value: scaled(k * step.mantissa, step.exponent),
            k,
        });
        k += 1;
    }
    out
}

fn push_grouped(out: &mut String, digits: &str, group: bool) {
    let n = digits.len();
    for (i, c) in digits.chars().enumerate() {
        if group && i > 0 && (n - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
}

/// The label of tick `k × step` under `format`: exact decimal digits, optional
/// thousands separators, prefix and suffix. `decimals` pads, or rounds half away from
/// zero.
pub fn tick_label(t: Tick, step: Step, format: &NumberFormat) -> String {
    let natural = (-step.exponent).max(0) as u32;
    let decimals = format.decimals.map_or(natural, u32::from);
    // The tick's exact value is (k × m) × 10^e; express it as an integer of
    // `decimals` fractional digits.
    let mut units: i128 = i128::from(t.k) * i128::from(step.mantissa);
    let shift = step.exponent + decimals as i32;
    if shift >= 0 {
        units *= 10i128.pow(shift as u32);
    } else {
        let d = 10i128.pow((-shift) as u32);
        let q = units / d;
        let r = units % d;
        units = if r.abs() * 2 >= d {
            q + units.signum()
        } else {
            q
        };
    }
    let mut out = String::new();
    if units < 0 {
        out.push('-');
    }
    out.push_str(&format.prefix);
    let mag = units.unsigned_abs();
    let p = 10u128.pow(decimals);
    let mut int = String::new();
    let _ = write!(int, "{}", mag / p);
    push_grouped(&mut out, &int, format.group);
    if decimals > 0 {
        let _ = write!(out, ".{:0width$}", mag % p, width = decimals as usize);
    }
    out.push_str(&format.suffix);
    out
}

/// A time tick and its label.
#[derive(Clone, Debug, PartialEq)]
pub struct TimeTick {
    pub ms: i64,
    pub label: String,
}

fn default_format(unit: TimeUnit, crosses_year: bool) -> &'static str {
    use TimeUnit::*;
    match unit {
        Millisecond | Second => "%H:%M:%S",
        Minute | Hour => "%H:%M",
        Day | Week if crosses_year => "%-d %b %Y",
        Day | Week => "%-d %b",
        Month if crosses_year => "%b %Y",
        Month => "%b",
        Year => "%Y",
    }
}

/// UTC ticks over `[lo, hi]` milliseconds for a plot `width` px long, at most `max`.
pub fn time_ticks(lo: i64, hi: i64, width: f64, format: Option<&str>, max: usize) -> Vec<TimeTick> {
    if hi <= lo {
        return Vec::new();
    }
    let interval: TickInterval =
        crate::layout::gantt::automatic_interval(i128::from(hi) - i128::from(lo), width);
    let crosses_year = calendar::date_parts(lo).year != calendar::date_parts(hi).year;
    let fmt = format.unwrap_or_else(|| default_format(interval.unit, crosses_year));
    let mut out = Vec::new();
    let Some(mut t) = crate::layout::gantt::first_aligned(lo, interval, 1) else {
        return out;
    };
    while t <= hi && out.len() < max {
        if let Some(label) = calendar::format_axis(t, fmt) {
            out.push(TimeTick { ms: t, label });
        }
        match calendar::add(t, i64::from(interval.count), interval.unit) {
            Some(n) if n > t => t = n,
            _ => break,
        }
    }
    out
}

/// A linear or time position: `v` in `[d0, d1]` mapped onto `[r0, r1]`.
pub fn map(v: f64, d0: f64, d1: f64, r0: f64, r1: f64) -> f64 {
    if d1 == d0 {
        return (r0 + r1) / 2.0;
    }
    r0 + (v - d0) / (d1 - d0) * (r1 - r0)
}

/// The data extent widened for a degenerate span: ±1, or ±10% of a nonzero value.
pub fn widen(lo: f64, hi: f64) -> (f64, f64) {
    if hi > lo {
        return (lo, hi);
    }
    let pad = if lo == 0.0 { 1.0 } else { abs(lo) / 10.0 };
    (lo - pad, hi + pad)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fmt() -> NumberFormat {
        NumberFormat {
            group: true,
            ..NumberFormat::default()
        }
    }

    #[test]
    fn steps_follow_one_two_five() {
        assert_eq!(
            nice_step(10.0, 5.0),
            Step {
                mantissa: 2,
                exponent: 0
            }
        );
        assert_eq!(
            nice_step(1.0, 5.0),
            Step {
                mantissa: 2,
                exponent: -1
            }
        );
        assert_eq!(
            nice_step(700.0, 5.0),
            Step {
                mantissa: 1,
                exponent: 2
            }
        );
        assert_eq!(
            nice_step(0.3, 6.0),
            Step {
                mantissa: 5,
                exponent: -2
            }
        );
    }

    #[test]
    fn domains_and_ticks_land_on_whole_steps() {
        let s = nice_step(0.3, 3.0);
        assert_eq!(
            s,
            Step {
                mantissa: 1,
                exponent: -1
            }
        );
        assert_eq!(nice_domain(0.3, 0.6, s), (0.3, 0.6));
        let t = ticks(0.3, 0.6, s, 64);
        let labels: Vec<_> = t.iter().map(|t| tick_label(*t, s, &fmt())).collect();
        assert_eq!(labels, ["0.3", "0.4", "0.5", "0.6"]);
        assert_eq!(
            nice_domain(
                -3.15,
                2.4,
                Step {
                    mantissa: 1,
                    exponent: 0
                }
            ),
            (-4.0, 3.0)
        );
    }

    #[test]
    fn labels_group_and_carry_affixes() {
        let s = Step {
            mantissa: 5,
            exponent: 1,
        };
        let t = Tick {
            value: 6550.0,
            k: 131,
        };
        assert_eq!(tick_label(t, s, &fmt()), "6,550");
        let t = Tick {
            value: 12500.0,
            k: 250,
        };
        assert_eq!(tick_label(t, s, &fmt()), "12,500");
        let f = NumberFormat {
            decimals: Some(1),
            prefix: String::from("$"),
            suffix: String::from("%"),
            group: true,
        };
        let neg = Tick { value: -1.0, k: -1 };
        assert_eq!(
            tick_label(
                neg,
                Step {
                    mantissa: 1,
                    exponent: 0
                },
                &f
            ),
            "-$1.0%"
        );
        let f0 = NumberFormat {
            decimals: Some(0),
            ..fmt()
        };
        let half = Tick { value: 2.5, k: 5 };
        assert_eq!(
            tick_label(
                half,
                Step {
                    mantissa: 5,
                    exponent: -1
                },
                &f0
            ),
            "3"
        );
    }

    #[test]
    fn time_ticks_are_aligned_utc_boundaries() {
        let lo = super::super::resolve::iso_time("2026-11-03").unwrap();
        let hi = super::super::resolve::iso_time("2026-11-28").unwrap();
        let t = time_ticks(lo, hi, 600.0, None, 64);
        assert!(!t.is_empty());
        assert!(t.iter().all(|t| t.ms >= lo && t.ms <= hi));
        assert!(t.iter().all(|t| t.ms % calendar::DAY_MS == 0));
    }
}
