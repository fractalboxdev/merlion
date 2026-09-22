//! Canonical number printing for SVG output (specs/architecture.md#determinism):
//! rounded to 1/100 px, no exponent, no trailing zeros, `-0` printed as `0`.

use alloc::string::String;
use core::fmt::Write;

/// Rounds half away from zero to hundredths without `f64::round` (std-only).
fn hundredths(v: f64) -> i64 {
    if !v.is_finite() {
        return 0;
    }
    let scaled = v * 100.0;
    // Clamp to the exactly-representable integer range; real coordinates are far smaller.
    let scaled = if scaled > 9.0e15 {
        9.0e15
    } else if scaled < -9.0e15 {
        -9.0e15
    } else {
        scaled
    };
    let t = scaled as i64; // truncates toward zero, saturates
    let frac = scaled - t as f64;
    if frac >= 0.5 {
        t + 1
    } else if frac <= -0.5 {
        t - 1
    } else {
        t
    }
}

pub fn push_num(out: &mut String, v: f64) {
    let h = hundredths(v);
    if h < 0 {
        out.push('-');
    }
    let a = h.unsigned_abs();
    let int = a / 100;
    let frac = a % 100;
    let _ = write!(out, "{}", int);
    if frac != 0 {
        if frac % 10 == 0 {
            let _ = write!(out, ".{}", frac / 10);
        } else {
            let _ = write!(out, ".{:02}", frac);
        }
    }
}

pub fn num(v: f64) -> String {
    let mut s = String::new();
    push_num(&mut s, v);
    s
}

#[cfg(test)]
mod tests {
    use super::num;

    #[test]
    fn canonical_forms() {
        assert_eq!(num(0.0), "0");
        assert_eq!(num(-0.0), "0");
        assert_eq!(num(-0.004), "0");
        assert_eq!(num(1.0), "1");
        assert_eq!(num(1.5), "1.5");
        assert_eq!(num(1.25), "1.25");
        assert_eq!(num(1.005), "1"); // 100.49999… after scaling
        assert_eq!(num(-3.456), "-3.46");
        assert_eq!(num(1e20), "90000000000000");
        assert_eq!(num(f64::NAN), "0");
        assert_eq!(num(123456.789), "123456.79");
    }
}
