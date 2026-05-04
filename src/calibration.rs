use core::ops::{Add, AddAssign, Div, Sub, SubAssign};

pub trait Int:
    Copy
    + PartialOrd
    + Add<Output = Self>
    + Sub<Output = Self>
    + Div<Output = Self>
    + AddAssign
    + SubAssign
{
    fn zero() -> Self;
    fn one() -> Self;

    fn saturating_sub(self, rhs: Self) -> Self;
    fn saturating_add(self, rhs: Self) -> Self;
}

impl Int for u16 {
    fn zero() -> Self {
        0
    }
    fn one() -> Self {
        1
    }

    fn saturating_sub(self, rhs: Self) -> Self {
        self.saturating_sub(rhs)
    }

    fn saturating_add(self, rhs: Self) -> Self {
        self.saturating_add(rhs)
    }
}

impl Int for i32 {
    fn zero() -> Self {
        0
    }
    fn one() -> Self {
        1
    }

    fn saturating_sub(self, rhs: Self) -> Self {
        self.saturating_sub(rhs)
    }

    fn saturating_add(self, rhs: Self) -> Self {
        self.saturating_add(rhs)
    }
}

pub struct AdaptiveRange<T: Int> {
    pub min: T,
    pub max: T,
    dirty: bool,
    threshold: T,
    learn_rate: Option<T>,
    decay_rate: Option<T>,
}

impl<T: Int> AdaptiveRange<T> {
    pub fn update(&mut self, value: T) {
        if self.decay_rate.is_some() {
            self.decay(value);
        }
        if self.learn_rate.is_some() {
            self.expand(value);
        }
    }

    fn expand(&mut self, value: T) {
        let expand_minimum = value < self.min.saturating_sub(self.threshold);
        if expand_minimum {
            let diff = self.min - value;
            self.min -= self.expand_step(diff);
        }

        let expand_maximum = value > self.max.saturating_add(self.threshold);
        if expand_maximum {
            let diff = value - self.max;
            self.max += self.expand_step(diff);
        }
    }

    fn decay(&mut self, value: T) {
        let decay_minimum = value > self.min.saturating_add(self.threshold);
        if decay_minimum {
            let diff = value - self.min;
            self.min += self.decay_step(diff);
        }

        let decay_maximum = value < self.max.saturating_sub(self.threshold);
        if decay_maximum {
            let diff = self.max - value;
            self.max -= self.decay_step(diff);
        }
    }

    fn expand_step(&self, diff: T) -> T {
        if let Some(learning_rate) = self.learn_rate {
            (diff + learning_rate - T::one()) / learning_rate
        } else {
            T::zero()
        }
    }

    fn decay_step(&self, diff: T) -> T {
        if let Some(decay_rate) = self.decay_rate {
            diff / decay_rate
        } else {
            T::zero()
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::calibration::AdaptiveRange;

    const NUMBER_OF_READINGS: u16 = u16::MAX;

    mod expanding_calibration_minimum {
        use super::*;

        #[test]
        fn when_reading_is_more_then_the_threshold_below_the_calibration_minimum() {
            // Given
            let initial_minimum = 100;
            let initial_maximum = 200;
            let mut calibration = AdaptiveRange {
                min: initial_minimum,
                max: initial_maximum,
                dirty: false,
                threshold: 8,
                learn_rate: Some(16),
                decay_rate: None,
            };

            let actual_min = initial_minimum - calibration.threshold - 1;

            // When
            for _ in 0..NUMBER_OF_READINGS {
                calibration.update(actual_min)
            }

            // Then
            assert!(
                initial_minimum > calibration.min,
                "Calibrated minimum ({}) is not lower then the initial minimum ({})",
                calibration.min,
                initial_minimum
            );
            assert!(
                actual_min <= calibration.min,
                "Calibrated minimum ({}) is not higher or equal then the actual minimum ({})",
                calibration.min,
                actual_min
            );
            assert_eq!(initial_maximum, calibration.max);
        }

        #[test]
        fn when_reading_is_less_then_the_threshold_below_the_calibration_minimum() {
            // Given
            let initial_minimum = 100;
            let mut calibration = AdaptiveRange {
                min: initial_minimum,
                max: 200,
                dirty: false,
                threshold: 8,
                learn_rate: Some(16),
                decay_rate: None,
            };

            let actual_min = initial_minimum - calibration.threshold + 1;

            // When
            for _ in 0..NUMBER_OF_READINGS {
                calibration.update(actual_min)
            }

            // Then
            assert_eq!(initial_minimum, calibration.min,);
        }

        #[test]
        fn when_reading_is_on_the_threshold_and_below_the_calibration_minimum() {
            // Given
            let initial_minimum = 100;
            let mut calibration = AdaptiveRange {
                min: initial_minimum,
                max: 200,
                dirty: false,
                threshold: 8,
                learn_rate: Some(16),
                decay_rate: None,
            };

            let actual_min = initial_minimum - calibration.threshold;

            // When
            for _ in 0..NUMBER_OF_READINGS {
                calibration.update(actual_min)
            }

            // Then
            assert_eq!(initial_minimum, calibration.min,);
        }
    }

    mod expanding_calibration_maximum {
        use super::*;

        #[test]
        fn when_reading_is_more_then_the_threshold_above_the_calibration_maximum() {
            // Given
            let initial_minimum = 100;
            let initial_maximum = 200;
            let mut calibration = AdaptiveRange {
                min: initial_minimum,
                max: initial_maximum,
                dirty: false,
                threshold: 8,
                learn_rate: Some(16),
                decay_rate: None,
            };

            let actual_max = initial_maximum + calibration.threshold + 1;

            // When
            for _ in 0..NUMBER_OF_READINGS {
                calibration.update(actual_max)
            }

            // Then
            assert!(
                initial_maximum < calibration.max,
                "Calibrated maximum ({}) is not higher then the initial maximum ({})",
                calibration.max,
                initial_maximum
            );
            assert!(
                actual_max >= calibration.max,
                "Calibrated maximum ({}) is not lower or equal then the actual minimum ({})",
                calibration.max,
                actual_max
            );
            assert_eq!(initial_minimum, calibration.min);
        }

        #[test]
        fn when_reading_is_less_then_the_threshold_above_the_calibration_maximum() {
            // Given
            let initial_maximum = 200;
            let mut calibration = AdaptiveRange {
                min: 100,
                max: initial_maximum,
                dirty: false,
                threshold: 8,
                learn_rate: Some(16),
                decay_rate: None,
            };

            let actual_max = initial_maximum + calibration.threshold - 1;

            // When
            for _ in 0..NUMBER_OF_READINGS {
                calibration.update(actual_max)
            }

            // Then
            assert_eq!(initial_maximum, calibration.max,)
        }

        #[test]
        fn when_reading_is_on_the_threshold_and_above_the_calibration_minimum() {
            // Given
            let initial_maximum = 200;
            let mut calibration = AdaptiveRange {
                min: 100,
                max: initial_maximum,
                dirty: false,
                threshold: 8,
                learn_rate: Some(16),
                decay_rate: None,
            };

            let actual_max = initial_maximum + calibration.threshold;

            // When
            for _ in 0..NUMBER_OF_READINGS {
                calibration.update(actual_max)
            }

            // Then
            assert_eq!(initial_maximum, calibration.max,)
        }
    }

    mod decay_calibration_minimum {
        use super::*;

        #[test]
        fn when_reading_is_more_then_the_decay_rate_above_the_calibration_minimum() {
            // Given
            let initial_minimum = 100;
            let initial_maximum = 200;
            let decay_rate = 1024;
            let mut calibration = AdaptiveRange {
                min: initial_minimum,
                max: initial_maximum,
                dirty: false,
                threshold: 8,
                learn_rate: None,
                decay_rate: Some(decay_rate),
            };

            let actual_min = initial_minimum + decay_rate;

            // When
            for _ in 0..NUMBER_OF_READINGS {
                calibration.update(actual_min)
            }

            // Then
            assert!(
                initial_minimum < calibration.min,
                "Calibrated minimum ({}) is not higher then the initial minimum ({})",
                calibration.min,
                initial_minimum
            );
            assert!(
                actual_min >= calibration.min,
                "Calibrated minimum ({}) is not lower or equal then the actual minimum ({})",
                calibration.min,
                actual_min
            );
            assert_eq!(initial_maximum, calibration.max);
        }

        #[test]
        fn when_reading_is_less_then_the_decay_rate_above_the_calibration_minimum() {
            // Given
            let initial_minimum = 100;
            let decay_rate = 1024;
            let mut calibration = AdaptiveRange {
                min: initial_minimum,
                max: 200,
                dirty: false,
                threshold: 8,
                learn_rate: None,
                decay_rate: Some(decay_rate),
            };

            let actual_min = initial_minimum + decay_rate - 1;

            // When
            for _ in 0..NUMBER_OF_READINGS {
                calibration.update(actual_min)
            }

            // Then
            assert_eq!(initial_minimum, calibration.min);
        }
    }

    mod decay_calibration_maximum {
        use super::*;

        #[test]
        fn when_reading_is_more_then_the_decay_rate_below_the_calibration_maximum() {
            // Given
            let initial_minimum = 100;
            let initial_maximum = 200;
            let decay_rate = 1024;
            let mut calibration = AdaptiveRange {
                min: initial_minimum,
                max: initial_maximum,
                dirty: false,
                threshold: 8,
                learn_rate: None,
                decay_rate: Some(decay_rate),
            };

            let actual_max = initial_maximum - decay_rate;

            // When
            for _ in 0..NUMBER_OF_READINGS {
                calibration.update(actual_max)
            }

            // Then
            assert!(
                initial_maximum > calibration.max,
                "Calibrated maximum ({}) is not lower then the initial maximum ({})",
                calibration.max,
                initial_maximum
            );
            assert!(
                actual_max <= calibration.max,
                "Calibrated maximum ({}) is not higher or equal then the actual maximum ({})",
                calibration.max,
                actual_max
            );
            assert_eq!(initial_minimum, calibration.min);
        }

        #[test]
        fn when_reading_is_less_then_the_decay_rate_below_the_calibration_maximum() {
            // Given
            let initial_maximum = 200;
            let decay_rate = 1024;
            let mut calibration = AdaptiveRange {
                min: 100,
                max: initial_maximum,
                dirty: false,
                threshold: 8,
                learn_rate: None,
                decay_rate: Some(decay_rate),
            };

            let actual_min = initial_maximum - decay_rate + 1;

            // When
            for _ in 0..NUMBER_OF_READINGS {
                calibration.update(actual_min)
            }

            // Then
            assert_eq!(initial_maximum, calibration.max);
        }
    }
}
