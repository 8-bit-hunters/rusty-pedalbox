use crate::calibration::{Int, Range};
use crate::fmt::{Format, debug, warn};
use crate::{LoadCell, Mapping};
use core::sync::atomic::{AtomicI16, Ordering};

pub struct LoadCellMonitorConfig<L, R, T>
where
    L: LoadCell<ReturnType = T>,
    R: Range<T>,
    T: Mapping + Int,
{
    pub range: R,
    pub load_cell: L,
    pub output_channel: &'static AtomicI16,
    pub raw_channel: Option<&'static T::Atomic>,
}

pub struct LoadCellMonitor<L, R, T>
where
    L: LoadCell<ReturnType = T>,
    R: Range<T>,
    T: Mapping + Int,
{
    _name: &'static str,
    range: R,
    load_cell: L,
    output_channel: &'static AtomicI16,
    raw_channel: Option<&'static T::Atomic>,
}

impl<L, R, T> LoadCellMonitor<L, R, T>
where
    L: LoadCell<ReturnType = T>,
    R: Range<T>,
    T: Mapping + Format + Int,
{
    pub fn new(
        name: &'static str,
        config: LoadCellMonitorConfig<L, R, T>,
    ) -> LoadCellMonitor<L, R, T> {
        Self {
            _name: name,
            range: config.range,
            load_cell: config.load_cell,
            output_channel: config.output_channel,
            raw_channel: config.raw_channel,
        }
    }

    pub fn run(&mut self) {
        if let Ok(raw_reading) = self.load_cell.read() {
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
        } else {
            warn!("Couldn't retrieve data")
        }
    }
}

#[cfg(test)]
mod load_cell_monitor_testing {
    use crate::LoadCell;
    use crate::calibration::Range;
    use crate::calibration::fixed::FixedRange;
    use crate::io_monitors::load_cell_monitor::{LoadCellMonitor, LoadCellMonitorConfig};
    use alloc::boxed::Box;
    use core::sync::atomic::{AtomicI16, AtomicI32, Ordering};
    use rstest::rstest;

    #[derive(Eq, PartialEq, Debug, Copy, Clone)]
    struct MockLoadCell {
        value: i32,
    }

    impl LoadCell for MockLoadCell {
        type ReturnType = i32;
        type Error = ();

        fn read(&mut self) -> Result<Self::ReturnType, Self::Error> {
            Ok(self.value)
        }
    }

    #[derive(Eq, PartialEq, Debug, Copy, Clone)]
    struct FailingLoadCell {}

    impl LoadCell for FailingLoadCell {
        type ReturnType = i32;
        type Error = ();

        fn read(&mut self) -> Result<Self::ReturnType, Self::Error> {
            Err(())
        }
    }

    #[test]
    fn when_creating_new_monitor() {
        // Given
        let name = "test";
        let range_min: i32 = 0;
        let range_max: i32 = 200;
        let load_cell = MockLoadCell { value: 100 };
        let range = FixedRange::default().min(range_min).max(range_max);
        let config = LoadCellMonitorConfig {
            range,
            load_cell,
            output_channel: Box::leak(Box::new(AtomicI16::default())),
            raw_channel: None,
        };

        // When
        let result = LoadCellMonitor::new(name, config);

        // Then
        assert_eq!(result._name, name);
        assert_eq!(result.range.get_min(), range_min);
        assert_eq!(result.range.get_max(), range_max);
        assert_eq!(result.load_cell, load_cell);
    }

    #[rstest]
    #[case::value_is_the_max(100, 0, 100, i16::MAX)]
    #[case::value_is_in_the_middle(50, 0, 100, -1)]
    #[case::value_is_the_min(0, 0, 100, i16::MIN)]
    #[case::value_is_over_the_max(101, 50, 100, i16::MAX)]
    #[case::value_is_under_the_min(49, 50, 100, i16::MIN)]
    fn when_mapping_to_range(
        #[case] value: i32,
        #[case] minimum: i32,
        #[case] maximum: i32,
        #[case] expected: i16,
    ) {
        // Given
        let load_cell = MockLoadCell { value };
        let output = Box::leak(Box::new(AtomicI16::default()));
        let range = FixedRange::default().min(minimum).max(maximum);
        let mut monitor = LoadCellMonitor::new(
            "test",
            LoadCellMonitorConfig {
                range,
                load_cell,
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
        let value: i32 = 100;
        let load_cell = MockLoadCell { value };
        let output = Box::leak(Box::new(AtomicI16::default()));
        let raw = Box::leak(Box::new(AtomicI32::default()));
        let range = FixedRange::default();
        let mut monitor = LoadCellMonitor::new(
            "test",
            LoadCellMonitorConfig {
                range,
                load_cell,
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
            "raw load cell reading should be published to the raw channel"
        );
    }

    #[test]
    fn when_reading_fails() {
        // Given
        let sentinel: i32 = 777;
        let output = Box::leak(Box::new(AtomicI16::default()));
        let raw = Box::leak(Box::new(AtomicI32::new(sentinel)));
        let range = FixedRange::default();
        let mut monitor = LoadCellMonitor::new(
            "test",
            LoadCellMonitorConfig {
                range,
                load_cell: FailingLoadCell {},
                output_channel: output,
                raw_channel: Some(raw),
            },
        );

        // When
        monitor.run();

        // Then
        assert_eq!(
            raw.load(Ordering::Relaxed),
            sentinel,
            "failed read must leave the raw channel unchanged"
        );
    }
}
