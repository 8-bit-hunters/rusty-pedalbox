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

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct ContractionConfig<T: Int> {
    contraction_rate: T,
    min_idle: u32,
    max_idle: u32,
    contraction_delay: u32,
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct ExpansionConfig<T: Int> {
    threshold: T,
    expansion_rate: T,
}

pub struct AdaptiveRange<T: Int> {
    pub min: T,
    pub max: T,
    dirty: bool,
    expansion_config: Option<ExpansionConfig<T>>,
    contraction_config: Option<ContractionConfig<T>>,
}

impl<T: Int> AdaptiveRange<T> {
    pub fn update(&mut self, value: T) {
        if self.contraction_config.is_some() {
            self.contract(value);
        }
        if self.expansion_config.is_some() {
            self.expand(value);
        }
    }

    fn expand(&mut self, value: T) {
        if let Some(expansion_config) = self.expansion_config {
            let expand_minimum = value < self.min.saturating_sub(expansion_config.threshold);
            if expand_minimum {
                let diff = self.min - value;
                self.min -= self.expand_step(diff);
            }

            let expand_maximum = value > self.max.saturating_add(expansion_config.threshold);
            if expand_maximum {
                let diff = value - self.max;
                self.max += self.expand_step(diff);
            }
        }
    }

    fn contract(&mut self, value: T) {
        if let Some(contraction_config) = self.contraction_config {
            let contract_minimum =
                value >= self.min.saturating_add(contraction_config.contraction_rate);
            if contract_minimum {
                let diff = value - self.min;
                self.min += self.contract_step(diff);
            }

            let contract_maximum =
                value <= self.max.saturating_sub(contraction_config.contraction_rate);
            if contract_maximum {
                let diff = self.max - value;
                self.max -= self.contract_step(diff);
            }
        }
    }

    fn expand_step(&self, diff: T) -> T {
        if let Some(expansion_config) = self.expansion_config {
            let expansion_rate = expansion_config.expansion_rate;
            (diff + expansion_rate - T::one()) / expansion_rate
        } else {
            T::zero()
        }
    }

    fn contract_step(&self, diff: T) -> T {
        if let Some(contraction_config) = self.contraction_config {
            diff / contraction_config.contraction_rate
        } else {
            T::zero()
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::calibration::AdaptiveRange;

    const NUMBER_OF_READINGS: u16 = u16::MAX;

    mod range_expansion_for_minimum {
        use super::*;
        use crate::calibration::ExpansionConfig;

        #[test]
        fn when_reading_is_more_then_the_threshold_below_the_calibration_minimum() {
            // Given
            let initial_minimum = 100;
            let initial_maximum = 200;
            let threshold = 8;
            let mut calibration = AdaptiveRange {
                min: initial_minimum,
                max: initial_maximum,
                dirty: false,
                expansion_config: Some(ExpansionConfig {
                    threshold,
                    expansion_rate: 16,
                }),
                contraction_config: None,
            };

            let actual_min = initial_minimum - threshold - 1;

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
            let threshold = 8;
            let mut calibration = AdaptiveRange {
                min: initial_minimum,
                max: 200,
                dirty: false,
                expansion_config: Some(ExpansionConfig {
                    threshold,
                    expansion_rate: 16,
                }),
                contraction_config: None,
            };

            let actual_min = initial_minimum - threshold + 1;

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
            let threshold = 8;
            let mut calibration = AdaptiveRange {
                min: initial_minimum,
                max: 200,
                dirty: false,
                expansion_config: Some(ExpansionConfig {
                    threshold,
                    expansion_rate: 16,
                }),
                contraction_config: None,
            };

            let actual_min = initial_minimum - threshold;

            // When
            for _ in 0..NUMBER_OF_READINGS {
                calibration.update(actual_min)
            }

            // Then
            assert_eq!(initial_minimum, calibration.min,);
        }
    }

    mod range_expansion_for_maximum {
        use super::*;
        use crate::calibration::ExpansionConfig;

        #[test]
        fn when_reading_is_more_then_the_threshold_above_the_calibration_maximum() {
            // Given
            let initial_minimum = 100;
            let initial_maximum = 200;
            let threshold = 8;
            let mut calibration = AdaptiveRange {
                min: initial_minimum,
                max: initial_maximum,
                dirty: false,
                expansion_config: Some(ExpansionConfig {
                    threshold,
                    expansion_rate: 16,
                }),
                contraction_config: None,
            };

            let actual_max = initial_maximum + threshold + 1;

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
            let threshold = 8;
            let mut calibration = AdaptiveRange {
                min: 100,
                max: initial_maximum,
                dirty: false,
                expansion_config: Some(ExpansionConfig {
                    threshold,
                    expansion_rate: 16,
                }),
                contraction_config: None,
            };

            let actual_max = initial_maximum + threshold - 1;

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
            let threshold = 8;
            let mut calibration = AdaptiveRange {
                min: 100,
                max: initial_maximum,
                dirty: false,
                expansion_config: Some(ExpansionConfig {
                    threshold,
                    expansion_rate: 16,
                }),
                contraction_config: None,
            };

            let actual_max = initial_maximum + threshold;

            // When
            for _ in 0..NUMBER_OF_READINGS {
                calibration.update(actual_max)
            }

            // Then
            assert_eq!(initial_maximum, calibration.max,)
        }
    }

    mod range_contraction_for_minimum {
        use super::*;
        use crate::calibration::{ContractionConfig, ExpansionConfig};

        #[test]
        fn when_reading_is_more_then_the_contract_rate_above_the_calibration_minimum() {
            // Given
            let initial_minimum = 100;
            let initial_maximum = 200;
            let contraction_rate = 1024;
            let mut calibration = AdaptiveRange {
                min: initial_minimum,
                max: initial_maximum,
                dirty: false,
                expansion_config: None,
                contraction_config: Some(ContractionConfig {
                    contraction_rate,
                    min_idle: 0,
                    max_idle: 0,
                    contraction_delay: 0,
                }),
            };

            let actual_min = initial_minimum + contraction_rate;

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
        fn when_reading_is_less_then_the_contract_rate_above_the_calibration_minimum() {
            // Given
            let initial_minimum = 100;
            let contraction_rate = 1024;
            let mut calibration = AdaptiveRange {
                min: initial_minimum,
                max: 200,
                dirty: false,
                expansion_config: None,
                contraction_config: Some(ContractionConfig {
                    contraction_rate,
                    min_idle: 0,
                    max_idle: 0,
                    contraction_delay: 0,
                }),
            };

            let actual_min = initial_minimum + contraction_rate - 1;

            // When
            for _ in 0..NUMBER_OF_READINGS {
                calibration.update(actual_min)
            }

            // Then
            assert_eq!(initial_minimum, calibration.min);
        }
    }

    mod range_contraction_for_maximum {
        use super::*;
        use crate::calibration::ContractionConfig;

        #[test]
        fn when_reading_is_more_then_the_contract_rate_below_the_calibration_maximum() {
            // Given
            let initial_minimum = 100;
            let initial_maximum = 200;
            let contraction_rate = 1024;
            let mut calibration = AdaptiveRange {
                min: initial_minimum,
                max: initial_maximum,
                dirty: false,
                expansion_config: None,
                contraction_config: Some(ContractionConfig {
                    contraction_rate,
                    min_idle: 0,
                    max_idle: 0,
                    contraction_delay: 0,
                }),
            };

            let actual_max = initial_maximum - contraction_rate;

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
        fn when_reading_is_less_then_the_contract_rate_below_the_calibration_maximum() {
            // Given
            let initial_maximum = 200;
            let contraction_rate = 1024;
            let mut calibration = AdaptiveRange {
                min: 100,
                max: initial_maximum,
                dirty: false,
                expansion_config: None,
                contraction_config: Some(ContractionConfig {
                    contraction_rate,
                    min_idle: 0,
                    max_idle: 0,
                    contraction_delay: 0,
                }),
            };

            let actual_min = initial_maximum - contraction_rate + 1;

            // When
            for _ in 0..NUMBER_OF_READINGS {
                calibration.update(actual_min)
            }

            // Then
            assert_eq!(initial_maximum, calibration.max);
        }
    }
}
