use crate::calibration::{Int, Range};

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct ContractionConfig<T: Int> {
    contraction_rate: T,
    contraction_delay: u32,
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct ExpansionConfig<T: Int> {
    threshold: T,
    expansion_rate: T,
}

#[derive(Debug, Default)]
pub struct AdaptiveRange<T: Int> {
    min: T,
    max: T,
    expansion_config: Option<ExpansionConfig<T>>,
    contraction_config: Option<ContractionConfig<T>>,
    dirty: bool,
    min_idle: u32,
    max_idle: u32,
}

impl<T: Int> AdaptiveRange<T> {
    pub fn min(self, value: T) -> AdaptiveRange<T> {
        Self {
            min: value,
            max: self.max,
            expansion_config: self.expansion_config,
            contraction_config: self.contraction_config,
            dirty: self.dirty,
            min_idle: self.min_idle,
            max_idle: self.max_idle,
        }
    }

    pub fn max(self, value: T) -> AdaptiveRange<T> {
        Self {
            min: self.min,
            max: value,
            expansion_config: self.expansion_config,
            contraction_config: self.contraction_config,
            dirty: self.dirty,
            min_idle: self.min_idle,
            max_idle: self.max_idle,
        }
    }

    pub fn expansion_config(self, value: ExpansionConfig<T>) -> AdaptiveRange<T> {
        Self {
            min: self.min,
            max: self.max,
            expansion_config: Some(value),
            contraction_config: self.contraction_config,
            dirty: self.dirty,
            min_idle: self.min_idle,
            max_idle: self.max_idle,
        }
    }

    pub fn contraction_config(self, value: ContractionConfig<T>) -> AdaptiveRange<T> {
        Self {
            min: self.min,
            max: self.max,
            expansion_config: self.expansion_config,
            contraction_config: Some(value),
            dirty: self.dirty,
            min_idle: self.min_idle,
            max_idle: self.max_idle,
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
            let value_is_above_threshold =
                value >= self.min.saturating_add(contraction_config.contraction_rate);
            let hitting_min_timed_out = self.min_idle >= contraction_config.contraction_delay;

            let contract_minimum = value_is_above_threshold && hitting_min_timed_out;
            if contract_minimum {
                let diff = value - self.min;
                self.min += self.contract_step(diff);
            }

            let value_is_above_threshold =
                value <= self.max.saturating_sub(contraction_config.contraction_rate);
            let hitting_max_timed_out = self.max_idle >= contraction_config.contraction_delay;

            let contract_maximum = value_is_above_threshold && hitting_max_timed_out;
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

    fn track_activity(&mut self, value: T) {
        if value < self.min {
            self.min_idle = 0;
        } else {
            self.min_idle += 1;
        }

        if value > self.max {
            self.max_idle = 0;
        } else {
            self.max_idle += 1;
        }
    }
}

impl<T: Int> Range<T> for AdaptiveRange<T> {
    fn get_min(&self) -> T {
        self.min
    }

    fn get_max(&self) -> T {
        self.max
    }

    fn update(&mut self, value: T) {
        if self.contraction_config.is_some() {
            self.track_activity(value);
            self.contract(value);
        }
        if self.expansion_config.is_some() {
            self.expand(value);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::calibration::adaptive::{AdaptiveRange, ContractionConfig, ExpansionConfig};
    use crate::calibration::Range;

    const NUMBER_OF_READINGS: u16 = u16::MAX;
    mod range_expansion_for_minimum {
        use super::*;

        #[test]
        fn when_reading_is_more_then_the_threshold_below_the_calibration_minimum() {
            // Given
            let initial_minimum = 100;
            let initial_maximum = 200;
            let threshold = 8;
            let mut calibration = AdaptiveRange::default()
                .min(initial_minimum)
                .max(initial_maximum)
                .expansion_config(ExpansionConfig {
                    threshold,
                    expansion_rate: 16,
                });

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
            let mut calibration = AdaptiveRange::default()
                .min(initial_minimum)
                .expansion_config(ExpansionConfig {
                    threshold,
                    expansion_rate: 16,
                });

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
            let mut calibration = AdaptiveRange::default()
                .min(initial_minimum)
                .expansion_config(ExpansionConfig {
                    threshold,
                    expansion_rate: 16,
                });

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

        #[test]
        fn when_reading_is_more_then_the_threshold_above_the_calibration_maximum() {
            // Given
            let initial_minimum = 100;
            let initial_maximum = 200;
            let threshold = 8;
            let mut calibration = AdaptiveRange::default()
                .min(initial_minimum)
                .max(initial_maximum)
                .expansion_config(ExpansionConfig {
                    threshold,
                    expansion_rate: 16,
                });

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
            let mut calibration = AdaptiveRange::default()
                .max(initial_maximum)
                .expansion_config(ExpansionConfig {
                    threshold,
                    expansion_rate: 16,
                });

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
            let mut calibration = AdaptiveRange::default()
                .max(initial_maximum)
                .expansion_config(ExpansionConfig {
                    threshold,
                    expansion_rate: 16,
                });

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

        #[test]
        fn when_reading_is_more_then_the_contract_rate_above_the_calibration_minimum() {
            // Given
            let initial_minimum = 100;
            let initial_maximum = 200;
            let contraction_rate = 1024;
            let mut calibration = AdaptiveRange::default()
                .min(initial_minimum)
                .max(initial_maximum)
                .contraction_config(ContractionConfig {
                    contraction_rate,
                    contraction_delay: 0,
                });

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
            let mut calibration = AdaptiveRange::default()
                .min(initial_minimum)
                .contraction_config(ContractionConfig {
                    contraction_rate,
                    contraction_delay: 0,
                });

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

        #[test]
        fn when_reading_is_more_then_the_contract_rate_below_the_calibration_maximum() {
            // Given
            let initial_minimum = 100;
            let initial_maximum = 200;
            let contraction_rate = 1024;
            let mut calibration = AdaptiveRange::default()
                .min(initial_minimum)
                .contraction_config(ContractionConfig {
                    contraction_rate,
                    contraction_delay: 0,
                });

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
            let mut calibration = AdaptiveRange::default()
                .max(initial_maximum)
                .contraction_config(ContractionConfig {
                    contraction_rate,
                    contraction_delay: 0,
                });

            let actual_min = initial_maximum - contraction_rate + 1;

            // When
            for _ in 0..NUMBER_OF_READINGS {
                calibration.update(actual_min)
            }

            // Then
            assert_eq!(initial_maximum, calibration.max);
        }
    }

    mod range_contraction_delay {
        use super::*;

        #[test]
        fn when_contraction_delay_is_used_on_max() {
            // Given
            let initial_maximum = 200;
            let contraction_rate = 1024;
            let contraction_delay = 200;

            let mut calibration = AdaptiveRange::default()
                .max(initial_maximum)
                .contraction_config(ContractionConfig {
                    contraction_rate,
                    contraction_delay,
                });

            let actual_max = initial_maximum - contraction_rate;

            let mut before_delay = None;
            let mut after_delay = None;

            // When
            for i in 0..NUMBER_OF_READINGS as usize {
                if i == (contraction_delay as usize - 1) {
                    before_delay = Some(calibration.max);
                }

                if i == (contraction_delay as usize) {
                    after_delay = Some(calibration.max);
                }

                calibration.update(actual_max);
            }

            // Then
            assert_eq!(before_delay.unwrap(), initial_maximum);

            assert!(
                after_delay.unwrap() < initial_maximum,
                "Max did not decay after delay"
            );
            assert!(
                after_delay.unwrap() >= actual_max,
                "Max decayed below observed value"
            );
        }

        #[test]
        fn when_contraction_delay_is_used_on_min() {
            // Given
            let initial_min = 100;
            let contraction_rate = 1024;
            let contraction_delay = 200;

            let mut calibration = AdaptiveRange::default()
                .min(initial_min)
                .contraction_config(ContractionConfig {
                    contraction_rate,
                    contraction_delay,
                });

            let actual_min = initial_min + contraction_rate;

            let mut before_delay = None;
            let mut after_delay = None;

            // When
            for i in 0..NUMBER_OF_READINGS as usize {
                if i == (contraction_delay as usize - 1) {
                    before_delay = Some(calibration.min);
                }

                if i == (contraction_delay as usize) {
                    after_delay = Some(calibration.min);
                }

                calibration.update(actual_min);
            }

            // Then
            assert_eq!(before_delay.unwrap(), initial_min);

            assert!(
                after_delay.unwrap() > initial_min,
                "Min did not decay after delay"
            );
            assert!(
                after_delay.unwrap() <= actual_min,
                "Min decayed above observed value"
            );
        }
    }
}
