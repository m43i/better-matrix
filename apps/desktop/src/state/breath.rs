//! The selected rail tile's breathing halo. It breathes for a few breaths after the user turns
//! to the window (focusing it, choosing a scope), then holds still: a loop redraws the whole
//! window at its frame rate, so an idle window that kept breathing would never rest.

/// How long one breath of the halo takes, in seconds.
pub const PERIOD: f64 = 5.0;

/// How many breaths it takes before it rests.
const BREATHS: f64 = 3.0;

/// Where the halo is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Breath {
    /// Asked to breathe; the next clock tick starts it.
    Asked,
    /// Breathing until the clock (seconds) reaches this, the end of a whole breath.
    Until(f64),
    /// Holding still.
    Resting,
}

impl Breath {
    /// Breathe again (anew when already breathing).
    pub fn ask(&mut self) {
        *self = Self::Asked;
    }

    /// Whether the halo breathes now.
    pub fn breathing(self) -> bool {
        self != Self::Resting
    }

    /// Moves on to `clock` (seconds); true when it came to rest, which changes the view. `wait`
    /// is how long until it should next be looked at, `None` once resting.
    pub fn advance(&mut self, clock: f64) -> (bool, Option<std::time::Duration>) {
        if *self == Self::Asked {
            // Rests at the end of a whole breath, where the halo looks as it does when still.
            *self = Self::Until(((clock / PERIOD).floor() + BREATHS + 1.0) * PERIOD);
        }
        match *self {
            Self::Until(end) if clock >= end => {
                *self = Self::Resting;
                (true, None)
            }
            Self::Until(end) => (false, Some(std::time::Duration::from_secs_f64(end - clock))),
            _ => (false, None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_breath_asked_for_should_last_a_few_whole_breaths() {
        let mut breath = Breath::Asked;
        breath.advance(1.0);
        assert_eq!(breath, Breath::Until(20.0));
    }

    #[test]
    fn a_breathing_halo_should_rest_at_its_end() {
        let mut breath = Breath::Until(20.0);
        assert_eq!(breath.advance(20.0), (true, None));
        assert!(!breath.breathing());
    }

    #[test]
    fn a_breathing_halo_should_wait_until_its_end() {
        let mut breath = Breath::Until(20.0);
        assert_eq!(breath.advance(12.5), (false, Some(std::time::Duration::from_secs_f64(7.5))));
        assert!(breath.breathing());
    }

    #[test]
    fn a_resting_halo_asked_again_should_breathe_again() {
        let mut breath = Breath::Resting;
        breath.ask();
        breath.advance(42.0);
        assert!(breath.breathing());
    }
}
