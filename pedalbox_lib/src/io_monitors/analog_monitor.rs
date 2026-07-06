use crate::calibration::{Int, Range};
use crate::fmt::{Format, debug};
use crate::{AnalogRead, Mapping};
use core::sync::atomic::{AtomicI16, Ordering};

pub struct AnalogMonitorConfig<Adc, Pin, R, T>
where
    Adc: AnalogRead<Pin, ReturnType = T>,
    R: Range<T>,
    T: Mapping + Int,
{
    pub range: R,
    pub adc: Adc,
    pub pin: Pin,
    pub output_channel: &'static AtomicI16,
    pub raw_channel: Option<&'static T::Atomic>,
}

pub struct AnalogMonitor<Adc, Pin, R, T>
where
    Adc: AnalogRead<Pin, ReturnType = T>,
    R: Range<T>,
    T: Mapping + Int,
{
    _name: &'static str,
    range: R,
    adc: Adc,
    pin: Pin,
    output_channel: &'static AtomicI16,
    raw_channel: Option<&'static T::Atomic>,
}

impl<Adc, Pin, R, T> AnalogMonitor<Adc, Pin, R, T>
where
    Adc: AnalogRead<Pin, ReturnType = T>,
    R: Range<T>,
    T: Mapping + Format + Int,
{
    pub fn new(
        name: &'static str,
        config: AnalogMonitorConfig<Adc, Pin, R, T>,
    ) -> AnalogMonitor<Adc, Pin, R, T> {
        Self {
            _name: name,
            adc: config.adc,
            pin: config.pin,
            range: config.range,
            output_channel: config.output_channel,
            raw_channel: config.raw_channel,
        }
    }

    pub fn run(&mut self) {
        let raw_reading = self.adc.read(&mut self.pin);
        if let Some(raw_channel) = self.raw_channel {
            raw_reading.store_in(raw_channel, Ordering::Relaxed);
        }

        self.range.update(raw_reading);

        let mapped_reading = raw_reading.map_to_i16(self.range.get_min(), self.range.get_max());
        self.output_channel.store(mapped_reading, Ordering::Relaxed);
        debug!(
            "Analog Monitor[{}]: Raw -> {}\tMapped -> {}",
            self._name, raw_reading, mapped_reading
        );
    }
}

#[cfg(test)]
mod analog_monitor_testing {
    use crate::AnalogRead;
    use crate::calibration::Range;
    use crate::calibration::fixed::FixedRange;
    use crate::io_monitors::analog_monitor::{AnalogMonitor, AnalogMonitorConfig};
    use alloc::boxed::Box;
    use core::sync::atomic::{AtomicI16, AtomicU16, Ordering};
    use rstest::rstest;

    #[derive(Eq, PartialEq, Debug, Copy, Clone)]
    struct MockAdc {}

    #[derive(Eq, PartialEq, Debug, Copy, Clone)]
    struct MockPin {
        pub value: u16,
    }

    impl AnalogRead<MockPin> for MockAdc {
        type ReturnType = u16;

        fn read(&mut self, pin: &mut MockPin) -> Self::ReturnType {
            pin.value
        }
    }

    #[test]
    fn when_creating_new_monitor() {
        // Given
        let name = "test";
        let adc = MockAdc {};
        let pin = MockPin { value: 100 };
        let range_min: u16 = 0;
        let range_max: u16 = 200;
        let range = FixedRange::default().min(range_min).max(range_max);

        let config = AnalogMonitorConfig {
            range,
            adc: adc.clone(),
            pin: pin.clone(),
            output_channel: Box::leak(Box::new(AtomicI16::default())),
            raw_channel: None,
        };

        // When
        let result = AnalogMonitor::new(name, config);

        // Then
        assert_eq!(result._name, name);
        assert_eq!(result.adc, adc);
        assert_eq!(result.pin, pin);
        assert_eq!(result.range.get_min(), range_min);
        assert_eq!(result.range.get_max(), range_max);
    }

    #[rstest]
    #[case::value_is_the_max(100, 0, 100, i16::MAX)]
    #[case::value_is_in_the_middle(50, 0, 100, -1)]
    #[case::value_is_the_min(0, 0, 100, i16::MIN)]
    #[case::value_is_over_the_max(201, 100, 200, i16::MAX)]
    #[case::value_is_under_the_min(99, 100, 200, i16::MIN)]
    fn when_mapping_to_range(
        #[case] value: u16,
        #[case] minimum: u16,
        #[case] maximum: u16,
        #[case] expected: i16,
    ) {
        // Given
        let adc = MockAdc {};
        let pin = MockPin { value };
        let output = Box::leak(Box::new(AtomicI16::default()));
        let range = FixedRange::default().min(minimum).max(maximum);

        let mut monitor = AnalogMonitor::new(
            "test",
            AnalogMonitorConfig {
                range,
                adc,
                pin,
                output_channel: output,
                raw_channel: None,
            },
        );

        // When
        monitor.run();

        // Then
        let result = output.load(Ordering::Relaxed);
        assert_eq!(result, expected);
    }

    #[test]
    fn when_running_with_configured_raw_channel() {
        // Given
        let value: u16 = 1234;
        let adc = MockAdc {};
        let pin = MockPin { value };
        let output = Box::leak(Box::new(AtomicI16::default()));
        let raw = Box::leak(Box::new(AtomicU16::default()));
        let range = FixedRange::default();

        let mut monitor = AnalogMonitor::new(
            "test",
            AnalogMonitorConfig {
                range,
                adc,
                pin,
                output_channel: output,
                raw_channel: Some(raw),
            },
        );

        // When
        monitor.run();

        // Then
        assert_eq!(
            raw.load(Ordering::Relaxed),
            value,
            "raw ADC reading should be published to the raw channel"
        );
    }
}
