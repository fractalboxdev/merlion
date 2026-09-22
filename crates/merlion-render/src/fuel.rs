//! Deterministic work budget (specs/adr/0008-deterministic-work-budget.md).
//! One unit is one step of a phase's inner loop.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutOfFuel;

#[derive(Clone, Debug)]
pub struct Fuel {
    limit: u64,
    used: u64,
    /// Units held back for mandatory phases; optional passes may not dip into them.
    reserve: u64,
}

impl Fuel {
    pub fn new(limit: u64) -> Self {
        Fuel {
            limit,
            used: 0,
            reserve: 0,
        }
    }

    pub fn used(&self) -> u64 {
        self.used
    }

    pub fn remaining(&self) -> u64 {
        self.limit.saturating_sub(self.used)
    }

    pub fn set_reserve(&mut self, units: u64) {
        self.reserve = units;
    }

    /// Mandatory work: may consume the reserve. Exhaustion means `TooLarge`.
    pub fn burn(&mut self, units: u64) -> Result<(), OutOfFuel> {
        let next = self.used.saturating_add(units);
        if next > self.limit {
            self.used = self.limit;
            return Err(OutOfFuel);
        }
        self.used = next;
        Ok(())
    }

    /// Optional work: fails (without consuming) once only the reserve is left.
    pub fn burn_optional(&mut self, units: u64) -> Result<(), OutOfFuel> {
        let next = self.used.saturating_add(units);
        if next > self.limit.saturating_sub(self.reserve) {
            return Err(OutOfFuel);
        }
        self.used = next;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mandatory_burn_fails_past_limit() {
        let mut f = Fuel::new(10);
        assert!(f.burn(10).is_ok());
        assert_eq!(f.burn(1), Err(OutOfFuel));
    }

    #[test]
    fn optional_burn_respects_reserve() {
        let mut f = Fuel::new(10);
        f.set_reserve(4);
        assert!(f.burn_optional(6).is_ok());
        assert_eq!(f.burn_optional(1), Err(OutOfFuel));
        assert_eq!(f.used(), 6);
        assert!(f.burn(4).is_ok());
    }
}
