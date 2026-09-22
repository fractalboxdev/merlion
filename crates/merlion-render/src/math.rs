//! Software float functions (specs/architecture.md#determinism). `core` lacks
//! `sqrt`/`atan2`/`sin`/`cos`/`hypot` on stable and zero dependencies rule out `libm`,
//! so these are implemented from integer and basic float operations and give
//! identical results on every target.
//!
//! STUB: owned by the layout workstream. `sqrt` must be correctly rounded; the others
//! to a documented error bound.

pub fn sqrt(x: f64) -> f64 {
    if !(x > 0.0) || !x.is_finite() {
        return if x == 0.0 || x.is_infinite() && x > 0.0 {
            x
        } else {
            f64::NAN
        };
    }
    let mut g = x;
    for _ in 0..64 {
        g = 0.5 * (g + x / g);
    }
    g
}

pub fn hypot(x: f64, y: f64) -> f64 {
    sqrt(x * x + y * y)
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

pub fn round(x: f64) -> f64 {
    if x >= 0.0 {
        floor(x + 0.5)
    } else {
        -floor(-x + 0.5)
    }
}
