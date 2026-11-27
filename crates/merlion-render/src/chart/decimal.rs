//! Exact decimal sums of written values (specs/chart.md#stacking).
//!
//! A stack's running totals and its printed total are sums of the values as written.
//! Summing their binary64 approximations would print `0.30000000000000004` for
//! `0.1 + 0.2` and could push a stack that ends exactly on an authored domain bound past
//! it, so each written value is read as an integer coefficient and a power of ten, and
//! sums are exact. A position is the correctly rounded binary64 value of the exact sum.

use alloc::string::String;
use alloc::vec::Vec;

/// `(-1)^neg × coefficient × 10^exp`; the coefficient's decimal digits are stored least
/// significant first, without zeros at either end. Zero has no digits.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Dec {
    neg: bool,
    digits: Vec<u8>,
    exp: i64,
}

/// A written value and the number of fractional digits it was written with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Written {
    pub value: Dec,
    pub places: u32,
}

impl Dec {
    pub fn zero() -> Dec {
        Dec::default()
    }

    pub fn is_zero(&self) -> bool {
        self.digits.is_empty()
    }

    pub fn is_negative(&self) -> bool {
        self.neg && !self.is_zero()
    }

    /// Reads a JSON number lexeme or a decimal string (`-?[0-9]+(\.[0-9]+)?`), with an
    /// optional exponent. `None` for any other text.
    pub fn parse(text: &str) -> Option<Written> {
        let b = text.as_bytes();
        let mut i = 0;
        let neg = b.first() == Some(&b'-');
        if neg {
            i += 1;
        }
        let int_start = i;
        while b.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        let int = &b[int_start..i];
        if int.is_empty() {
            return None;
        }
        let mut frac: &[u8] = &[];
        if b.get(i) == Some(&b'.') {
            i += 1;
            let s = i;
            while b.get(i).is_some_and(u8::is_ascii_digit) {
                i += 1;
            }
            frac = &b[s..i];
            if frac.is_empty() {
                return None;
            }
        }
        let mut exp: i64 = 0;
        if matches!(b.get(i), Some(b'e' | b'E')) {
            i += 1;
            let minus = match b.get(i) {
                Some(b'-') => {
                    i += 1;
                    true
                }
                Some(b'+') => {
                    i += 1;
                    false
                }
                _ => false,
            };
            let s = i;
            while let Some(d) = b.get(i).filter(|c| c.is_ascii_digit()) {
                // Saturates far beyond any exponent a finite binary64 value can need.
                exp = (exp * 10 + i64::from(d - b'0')).min(1 << 40);
                i += 1;
            }
            if i == s {
                return None;
            }
            if minus {
                exp = -exp;
            }
        }
        if i != b.len() {
            return None;
        }
        let frac_len = i64::try_from(frac.len()).ok()?;
        // A zero may carry any exponent (`0e-99999`); its precision saturates, and a total
        // that would print it fails the drawn text limit instead.
        let places = u32::try_from((frac_len - exp).max(0)).unwrap_or(u32::MAX);
        let mut digits: Vec<u8> = int
            .iter()
            .chain(frac.iter())
            .rev()
            .map(|d| d - b'0')
            .collect();
        let mut exp = exp - frac_len;
        let low = digits.iter().take_while(|&&d| d == 0).count();
        digits.drain(..low);
        exp += i64::try_from(low).ok()?;
        while digits.last() == Some(&0) {
            digits.pop();
        }
        if digits.is_empty() {
            return Some(Written {
                value: Dec::zero(),
                places,
            });
        }
        Some(Written {
            value: Dec { neg, digits, exp },
            places,
        })
    }

    /// The exact sum `self + other`.
    pub fn add(&self, other: &Dec) -> Dec {
        if other.is_zero() {
            return self.clone();
        }
        if self.is_zero() {
            return other.clone();
        }
        let exp = self.exp.min(other.exp);
        let a = self.aligned(exp);
        let b = other.aligned(exp);
        let (neg, digits) = if self.neg == other.neg {
            (self.neg, add_digits(&a, &b))
        } else {
            match cmp_digits(&a, &b) {
                core::cmp::Ordering::Equal => return Dec::zero(),
                core::cmp::Ordering::Greater => (self.neg, sub_digits(&a, &b)),
                core::cmp::Ordering::Less => (other.neg, sub_digits(&b, &a)),
            }
        };
        normalise(neg, digits, exp)
    }

    /// The coefficient's digits for a lower exponent `exp`, least significant first.
    fn aligned(&self, exp: i64) -> Vec<u8> {
        let shift = usize::try_from(self.exp - exp).unwrap_or(0);
        let mut v = alloc::vec![0u8; shift];
        v.extend_from_slice(&self.digits);
        v
    }

    /// The correctly rounded binary64 value.
    pub fn to_f64(&self) -> f64 {
        if self.is_zero() {
            return 0.0;
        }
        let mut s = String::with_capacity(self.digits.len() + 24);
        if self.neg {
            s.push('-');
        }
        for &d in self.digits.iter().rev() {
            s.push(char::from(b'0' + d));
        }
        s.push('e');
        let _ = core::fmt::Write::write_fmt(&mut s, format_args!("{}", self.exp));
        s.parse::<f64>().unwrap_or(f64::NAN)
    }

    /// The position of the leading digit of the plain notation: 0 below one.
    fn top(&self) -> i64 {
        if self.is_zero() {
            0
        } else {
            (self.exp + self.digits.len() as i64 - 1).max(0)
        }
    }

    /// The length in bytes of [`Self::to_plain`] with `places`, without building it.
    pub fn plain_len(&self, places: u32) -> u64 {
        let int = self.top() as u64 + 1;
        let frac = if places > 0 { u64::from(places) + 1 } else { 0 };
        u64::from(self.is_negative()) + int + frac
    }

    /// Plain decimal notation with exactly `places` fractional digits; `places` is at
    /// least the number of fractional digits the value has. Zero has no sign. Callers
    /// bound [`Self::plain_len`] first.
    pub fn to_plain(&self, places: u32) -> String {
        let places = i64::from(places);
        let mut out = String::new();
        if self.is_negative() {
            out.push('-');
        }
        // Digit at position p (value 10^p) for p from the top down to -places.
        let top = self.top();
        let digit = |p: i64| -> u8 {
            let k = p - self.exp;
            if k < 0 {
                return 0;
            }
            usize::try_from(k)
                .ok()
                .and_then(|k| self.digits.get(k))
                .copied()
                .unwrap_or(0)
        };
        let mut p = top;
        while p >= -places {
            if p == -1 {
                out.push('.');
            }
            out.push(char::from(b'0' + digit(p)));
            p -= 1;
        }
        out
    }
}

fn normalise(neg: bool, mut digits: Vec<u8>, mut exp: i64) -> Dec {
    let low = digits.iter().take_while(|&&d| d == 0).count();
    digits.drain(..low);
    exp += low as i64;
    while digits.last() == Some(&0) {
        digits.pop();
    }
    if digits.is_empty() {
        return Dec::zero();
    }
    Dec { neg, digits, exp }
}

fn add_digits(a: &[u8], b: &[u8]) -> Vec<u8> {
    let n = a.len().max(b.len());
    let mut out = Vec::with_capacity(n + 1);
    let mut carry = 0u8;
    for i in 0..n {
        let s = a.get(i).copied().unwrap_or(0) + b.get(i).copied().unwrap_or(0) + carry;
        out.push(s % 10);
        carry = s / 10;
    }
    if carry > 0 {
        out.push(carry);
    }
    out
}

/// `a - b` for `a >= b`.
fn sub_digits(a: &[u8], b: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(a.len());
    let mut borrow = 0i8;
    for (i, &d) in a.iter().enumerate() {
        let mut v = d as i8 - b.get(i).copied().unwrap_or(0) as i8 - borrow;
        borrow = 0;
        if v < 0 {
            v += 10;
            borrow = 1;
        }
        out.push(v as u8);
    }
    out
}

fn cmp_digits(a: &[u8], b: &[u8]) -> core::cmp::Ordering {
    let len = |v: &[u8]| v.len() - v.iter().rev().take_while(|&&d| d == 0).count();
    let (la, lb) = (len(a), len(b));
    if la != lb {
        return la.cmp(&lb);
    }
    for i in (0..la).rev() {
        match a[i].cmp(&b[i]) {
            core::cmp::Ordering::Equal => continue,
            o => return o,
        }
    }
    core::cmp::Ordering::Equal
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sum(values: &[&str]) -> (Dec, u32) {
        let mut total = Dec::zero();
        let mut places = 0;
        for v in values {
            let w = Dec::parse(v).unwrap();
            places = places.max(w.places);
            total = total.add(&w.value);
        }
        (total, places)
    }

    #[test]
    fn sums_are_exact_and_keep_the_written_precision() {
        let (t, p) = sum(&["0.1", "0.2"]);
        assert_eq!(t.to_plain(p), "0.3");
        assert_eq!(t.to_f64(), 0.3);
        let (t, p) = sum(&["12.50", "3.2", "-0.70"]);
        assert_eq!(t.to_plain(p), "15.00");
        let (t, p) = sum(&["-2.40", "1.1"]);
        assert_eq!(t.to_plain(p), "-1.30");
        let (t, p) = sum(&["33.3", "33.3", "33.4"]);
        assert_eq!((t.to_plain(p).as_str(), t.to_f64()), ("100.0", 100.0));
        let (t, p) = sum(&["1.5", "-1.5"]);
        assert_eq!(t.to_plain(p), "0.0");
        let (t, p) = sum(&["-0.0"]);
        assert_eq!(t.to_plain(p), "0.0");
    }

    #[test]
    fn exponents_and_leading_zeros_read_as_written() {
        let (t, p) = sum(&["1.5e2", "15e-1", "007"]);
        assert_eq!(t.to_plain(p), "158.5");
        let (t, p) = sum(&["1e300", "1"]);
        assert_eq!(t.to_f64(), 1e300);
        assert_eq!(t.to_plain(p).len(), 301);
        let (t, p) = sum(&["5e-324"]);
        assert_eq!(t.to_f64(), f64::from_bits(1));
        assert_eq!(p, 324);
        for (values, plain) in [
            (&["-12.5", "0.25"][..], "-12.25"),
            (&["0.5"][..], "0.5"),
            (&["0"][..], "0"),
            (&["1e300"][..], ""),
        ] {
            let (t, p) = sum(values);
            assert_eq!(t.plain_len(p), t.to_plain(p).len() as u64, "{values:?}");
            if !plain.is_empty() {
                assert_eq!(t.to_plain(p), plain);
            }
        }
        let zero = Dec::parse("0e-99999999999999").unwrap();
        assert!(zero.value.is_zero());
        assert_eq!(zero.places, u32::MAX);
        for bad in ["", "-", "1.", ".5", "1e", "+1", "1,5", " 1"] {
            assert_eq!(Dec::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn subtraction_borrows_across_digits() {
        let (t, p) = sum(&["1000", "-0.001"]);
        assert_eq!(t.to_plain(p), "999.999");
        let (t, p) = sum(&["-1000", "0.001"]);
        assert_eq!(t.to_plain(p), "-999.999");
    }
}
