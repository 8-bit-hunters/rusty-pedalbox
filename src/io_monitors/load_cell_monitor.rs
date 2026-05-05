use crate::calibration::{Int, Range};
#[cfg(target_arch = "x86_64")]
use crate::fmt::defmt::Format;
use crate::fmt::{debug, warn};
use crate::{LoadCell, Mapping};
use core::sync::atomic::{AtomicI16, Ordering};
#[cfg(all(target_arch = "arm", feature = "defmt"))]
use defmt::Format;

pub struct LoadCellMonitorConfig<L, R, T>
where
    L: LoadCell<ReturnType = T>,
    R: Range<T>,
    T: Mapping + Int,
{
    pub range: R,
    pub load_cell: L,
    pub output_channel: &'static AtomicI16,
}

pub struct LoadCellMonitor<L, R, T>
where
    L: LoadCell<ReturnType = T>,
    R: Range<T>,
    T: Mapping + Int,
{
    name: &'static str,
    range: R,
    load_cell: L,
    output_channel: &'static AtomicI16,
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
            name,
            range: config.range,
            load_cell: config.load_cell,
            output_channel: config.output_channel,
        }
    }

    pub fn run(&mut self) {
        match self.load_cell.read() {
            Ok(raw_reading) => {
                self.range.update(raw_reading);

                let mapped_reading =
                    raw_reading.map_to_i16(self.range.get_min(), self.range.get_max());
                self.output_channel.store(mapped_reading, Ordering::Relaxed);
                debug!(
                    "Analog Monitor[{}]: Raw -> {}\tMapped -> {}",
                    self.name, raw_reading, mapped_reading
                );
            }
            Err(_) => {
                warn!("Couldn't retrieve data")
            }
        }
    }
}

#[cfg(test)]
mod load_cell_monitor_testing {
    use crate::calibration::fixed::FixedRange;
    use crate::calibration::Range;
    use crate::io_monitors::load_cell_monitor::{LoadCellMonitor, LoadCellMonitorConfig};
    use crate::LoadCell;
    use alloc::boxed::Box;
    use core::sync::atomic::{AtomicI16, Ordering};
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
        };

        // When
        let result = LoadCellMonitor::new(name, config);

        // Then
        assert_eq!(result.name, name);
        assert_eq!(result.range.get_min(), range_min);
        assert_eq!(result.range.get_max(), range_max);
        assert_eq!(result.load_cell, load_cell);
    }

    #[rstest]
    #[case(100, 0, 100, i16::MAX)]
    #[case(50, 0, 100, -1)]
    #[case(0, 0, 100, i16::MIN)]
    #[case(101, 50, 100, i16::MAX)]
    #[case(49, 50, 100, i16::MIN)]
    fn when_value_inside_the_input_range(
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
            },
        );

        // When
        monitor.run();

        // Then
        let result = output.load(Ordering::Relaxed);
        assert_eq!(result, expected);
    }
}
