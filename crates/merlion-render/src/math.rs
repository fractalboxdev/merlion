//! Software float functions (specs/architecture.md#determinism). `core` lacks
//! `sqrt`/`hypot` on stable and zero dependencies rule out `libm`, so these are
//! implemented from integer and basic float operations and give identical results on
//! every target.
//!
//! - [`sqrt`] is correctly rounded (round to nearest, ties to even): the same result as
//!   the IEEE 754 hardware instruction.
//! - [`hypot`] is within 2 ulp of the exact result and never overflows or underflows
//!   in its intermediate steps.
//! - [`floor`], [`ceil`], [`round`], [`abs`], [`min`], [`max`], [`clamp`] are exact.
//!
//! The layout needs no trigonometry: every angle it uses is a multiple of 90° or comes
//! from a vector that is normalised with [`hypot`].

/// Integer square root: the largest `r` with `r * r <= n` (bit-by-bit method, exact).
fn isqrt_u128(n: u128) -> u128 {
    let mut rem = n;
    let mut root: u128 = 0;
    // Highest power of four not above n.
    let mut bit: u128 = 1u128 << 126;
    while bit > rem {
        bit >>= 2;
    }
    while bit != 0 {
        if rem >= root + bit {
            rem -= root + bit;
            root = (root >> 1) + bit;
        } else {
            root >>= 1;
        }
        bit >>= 2;
    }
    root
}

/// Correctly rounded square root.
///
/// `x = m · 2^e` with integer `m`; the mantissa is scaled by an even power of two to
/// about 2^112 so that the integer root has 56 significant bits, and the remainder of
/// the integer root is the sticky bit for round-to-nearest-even.
pub fn sqrt(x: f64) -> f64 {
    if x.is_nan() || x < 0.0 {
        return f64::NAN;
    }
    if x == 0.0 || x.is_infinite() {
        return x; // keeps the sign of -0.0
    }
    let bits = x.to_bits();
    let biased = ((bits >> 52) & 0x7ff) as i32;
    let frac = bits & ((1u64 << 52) - 1);
    // x = m · 2^e exactly.
    let (m, e): (u128, i32) = if biased == 0 {
        (frac as u128, -1074)
    } else {
        ((frac | (1u64 << 52)) as u128, biased - 1075)
    };
    // Normalise m to 2^111 <= M < 2^113 with an even shift and an even exponent.
    let lz = m.leading_zeros() as i32; // m < 2^53, so lz >= 75
    let mut shift = lz - 15; // puts the top bit at position 112
    let mut exp = e - shift;
    if exp & 1 != 0 {
        shift -= 1;
        exp += 1;
    }
    let big = m << shift as u32;
    let root = isqrt_u128(big); // 2^55 <= root < 2^56.5
    let sticky = root * root != big;
    // Value = (root + frac) · 2^(exp/2); round root to 53 bits.
    let rbits = 128 - root.leading_zeros() as i32;
    let drop = rbits - 53;
    let mut mant = root >> drop as u32;
    let rest = root & ((1u128 << drop as u32) - 1);
    let half = 1u128 << (drop - 1) as u32;
    let round_up = rest > half || (rest == half && (sticky || mant & 1 == 1));
    let mut e2 = exp / 2 + drop;
    if round_up {
        mant += 1;
        if mant == 1u128 << 53 {
            mant >>= 1;
            e2 += 1;
        }
    }
    // Result = mant · 2^e2 with 2^52 <= mant < 2^53; always a normal number.
    let biased_out = (e2 + 52 + 1023) as u64;
    f64::from_bits((biased_out << 52) | ((mant as u64) & ((1u64 << 52) - 1)))
}

/// `sqrt(x² + y²)` within 2 ulp. Operands outside [2^-500, 2^500] are scaled by an
/// exact power of two first, so squaring neither overflows nor underflows.
pub fn hypot(x: f64, y: f64) -> f64 {
    let (mut a, mut b) = (abs(x), abs(y));
    if a.is_infinite() || b.is_infinite() {
        return f64::INFINITY;
    }
    if a.is_nan() || b.is_nan() {
        return f64::NAN;
    }
    if a < b {
        core::mem::swap(&mut a, &mut b);
    }
    if a == 0.0 {
        return 0.0;
    }
    const BIG: f64 = f64::from_bits((1023 + 500) << 52); // 2^500
    const SMALL: f64 = f64::from_bits((1023 - 500) << 52); // 2^-500
    let scale = if a > BIG {
        SMALL
    } else if a < SMALL {
        BIG
    } else {
        1.0
    };
    let (sa, sb) = (a * scale, b * scale);
    sqrt(sa * sa + sb * sb) / scale
}

pub fn abs(x: f64) -> f64 {
    f64::from_bits(x.to_bits() & !(1u64 << 63))
}

pub fn floor(x: f64) -> f64 {
    if !x.is_finite() || abs(x) >= 4_503_599_627_370_496.0 {
        return x;
    }
    let t = x as i64 as f64;
    if t > x {
        t - 1.0
    } else {
        t
    }
}

pub fn ceil(x: f64) -> f64 {
    -floor(-x)
}

/// Rounds half away from zero.
pub fn round(x: f64) -> f64 {
    if x >= 0.0 {
        floor(x + 0.5)
    } else {
        -floor(-x + 0.5)
    }
}

/// The smaller operand; `a` when they compare equal or either is NaN.
pub fn min(a: f64, b: f64) -> f64 {
    if b < a {
        b
    } else {
        a
    }
}

/// The larger operand; `a` when they compare equal or either is NaN.
pub fn max(a: f64, b: f64) -> f64 {
    if b > a {
        b
    } else {
        a
    }
}

/// `v` limited to `[lo, hi]`; NaN maps to `lo`.
pub fn clamp(v: f64, lo: f64, hi: f64) -> f64 {
    if v.is_nan() || v < lo {
        lo
    } else if v > hi {
        hi
    } else {
        v
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;

    /// Deterministic 64-bit LCG (Knuth MMIX constants).
    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            self.0
        }
    }

    /// Exact check that `r` is the correctly rounded square root of `x` (both positive,
    /// finite): `x` lies between the squares of the midpoints to `r`'s neighbours.
    fn is_correctly_rounded(x: f64, r: f64) -> bool {
        // Decompose into integer mantissa and exponent.
        fn split(v: f64) -> (u128, i32) {
            let b = v.to_bits();
            let e = ((b >> 52) & 0x7ff) as i32;
            let m = b & ((1u64 << 52) - 1);
            if e == 0 {
                (m as u128, -1074)
            } else {
                ((m | (1u64 << 52)) as u128, e - 1075)
            }
        }
        let (rm, re) = split(r);
        let (xm, xe) = split(x);
        // Midpoints (2R±1)·2^(re-1); squares (2R±1)²·2^(2re-2). Compare with xm·2^xe.
        let hi = (2 * rm + 1) * (2 * rm + 1);
        let lo = (2 * rm - 1) * (2 * rm - 1);
        let shift = xe - (2 * re - 2);
        let cmp = |sq: u128| -> core::cmp::Ordering {
            // Compare sq with xm·2^shift.
            if shift >= 0 {
                sq.cmp(&(xm << shift as u32))
            } else {
                (sq << (-shift) as u32).cmp(&xm)
            }
        };
        cmp(lo) != core::cmp::Ordering::Greater && cmp(hi) != core::cmp::Ordering::Less
    }

    #[test]
    fn sqrt_of_perfect_squares_is_exact() {
        for i in 0u64..2000 {
            let f = i as f64;
            assert_eq!(sqrt(f * f), f, "sqrt({})", f * f);
        }
        assert_eq!(sqrt(0.25), 0.5);
        assert_eq!(sqrt(1e300 * 1e-300 * 4.0), 2.0);
    }

    #[test]
    fn sqrt_special_values() {
        assert!(sqrt(-1.0).is_nan());
        assert!(sqrt(f64::NAN).is_nan());
        assert_eq!(sqrt(f64::INFINITY), f64::INFINITY);
        assert!(sqrt(f64::NEG_INFINITY).is_nan());
        assert_eq!(sqrt(0.0).to_bits(), 0.0f64.to_bits());
        assert_eq!(sqrt(-0.0).to_bits(), (-0.0f64).to_bits());
    }

    #[test]
    fn sqrt_is_correctly_rounded_on_random_inputs() {
        let mut g = Lcg(42);
        for _ in 0..20_000 {
            let bits = g.next() & 0x7fef_ffff_ffff_ffff; // positive, finite
            let x = f64::from_bits(bits);
            if x == 0.0 {
                continue;
            }
            let r = sqrt(x);
            assert!(is_correctly_rounded(x, r), "sqrt({:e}) = {:e}", x, r);
            assert_eq!(
                r.to_bits(),
                x.sqrt().to_bits(),
                "hardware disagrees at {:e}",
                x
            );
        }
    }

    #[test]
    fn sqrt_is_correctly_rounded_on_subnormals_and_extremes() {
        for x in [
            f64::from_bits(1),
            f64::from_bits(3),
            f64::from_bits(0x000f_ffff_ffff_ffff),
            f64::MIN_POSITIVE,
            f64::MAX,
            2.0,
            3.0,
            1.0 - f64::EPSILON / 2.0,
            1.0 + f64::EPSILON,
        ] {
            let r = sqrt(x);
            assert!(is_correctly_rounded(x, r), "sqrt({:e})", x);
            assert_eq!(r.to_bits(), x.sqrt().to_bits());
        }
    }

    #[test]
    fn hypot_is_within_two_ulp_and_does_not_overflow() {
        assert_eq!(hypot(3.0, 4.0), 5.0);
        assert_eq!(hypot(-5.0, 12.0), 13.0);
        assert_eq!(hypot(0.0, 0.0), 0.0);
        assert_eq!(hypot(1e300, 1e300), 1e300 * core::f64::consts::SQRT_2);
        let mut g = Lcg(7);
        for _ in 0..5000 {
            let a = (g.next() >> 11) as f64 / (1u64 << 40) as f64 - 2048.0;
            let b = (g.next() >> 11) as f64 / (1u64 << 40) as f64 - 2048.0;
            let h = hypot(a, b);
            let e = a.hypot(b);
            let ulp = f64::from_bits(e.to_bits() + 1) - e;
            assert!((h - e).abs() <= 2.0 * ulp, "hypot({}, {})", a, b);
        }
    }

    #[test]
    fn floor_ceil_round_abs() {
        assert_eq!(floor(-1.5), -2.0);
        assert_eq!(floor(1.5), 1.0);
        assert_eq!(ceil(1.2), 2.0);
        assert_eq!(round(2.5), 3.0);
        assert_eq!(round(-2.5), -3.0);
        assert_eq!(abs(-3.0), 3.0);
        assert_eq!(min(1.0, 2.0), 1.0);
        assert_eq!(max(1.0, 2.0), 2.0);
        assert_eq!(clamp(5.0, 0.0, 3.0), 3.0);
        assert_eq!(clamp(f64::NAN, 0.0, 3.0), 0.0);
    }
}
