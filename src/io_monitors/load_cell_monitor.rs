#[cfg(target_arch = "x86_64")]
use crate::fmt::defmt::Format;
use crate::fmt::{debug, warn};
use crate::{LoadCell, Mapping};
use core::sync::atomic::{AtomicI16, Ordering};
#[cfg(target_arch = "arm")]
use defmt::Format;

pub struct LoadCellMonitorConfig<L, T>
where
    L: LoadCell<ReturnType = T>,
    T: Mapping,
{
    pub range_min: T,
    pub range_max: T,
    pub load_cell: L,
    pub output_channel: &'static AtomicI16,
}

pub struct LoadCellMonitor<L, T>
where
    L: LoadCell<ReturnType = T>,
    T: Mapping,
{
    name: &'static str,
    range_min: T,
    range_max: T,
    load_cell: L,
    output_channel: &'static AtomicI16,
}

impl<L, T> LoadCellMonitor<L, T>
where
    L: LoadCell<ReturnType = T>,
    T: Mapping + Format,
{
    pub fn new(name: &'static str, config: LoadCellMonitorConfig<L, T>) -> LoadCellMonitor<L, T> {
        Self {
            name,
            range_min: config.range_min,
            range_max: config.range_max,
            load_cell: config.load_cell,
            output_channel: config.output_channel,
        }
    }

    pub fn run(&mut self) {
        match self.load_cell.read() {
            Ok(raw_reading) => {
                let mapped_reading = raw_reading.map_to_i16(self.range_min, self.range_max);
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
        let config = LoadCellMonitorConfig {
            range_min,
            range_max,
            load_cell,
            output_channel: Box::leak(Box::new(AtomicI16::default())),
        };

        // When
        let result = LoadCellMonitor::new(name, config);

        // Then
        assert_eq!(result.name, name);
        assert_eq!(result.range_min, range_min);
        assert_eq!(result.range_max, range_max);
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
        let mut monitor = LoadCellMonitor::new(
            "test",
            LoadCellMonitorConfig {
                range_min: minimum,
                range_max: maximum,
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
