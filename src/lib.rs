#![cfg_attr(not(test), no_std)]

#[cfg(test)]
extern crate alloc;
#[cfg(target_arch = "arm")]
use embassy_stm32::adc::Adc;
#[cfg(target_arch = "arm")]
use embedded_hal::blocking::delay::DelayUs;
#[cfg(target_arch = "arm")]
use embedded_hal::digital::v2::InputPin;
#[cfg(target_arch = "arm")]
use embedded_hal::digital::v2::OutputPin;
#[cfg(target_arch = "arm")]
use hx711::Hx711;

pub mod fmt;
mod io_monitor;

pub mod prelude {
    pub use super::fmt::*;
    pub use super::io_monitor::{
        AnalogMonitor, AnalogMonitorConfig, LoadCellMonitor, LoadCellMonitorConfig,
    };
    pub use super::{AnalogRead, Mapping};
}

pub trait LoadCell {
    type ReturnType;
    type Error;
    fn read(&mut self) -> Result<Self::ReturnType, Self::Error>;
}

#[cfg(target_arch = "arm")]
impl<D, IN, OUT, EIN, EOUT> LoadCell for Hx711<D, IN, OUT>
where
    D: DelayUs<u32>,
    IN: InputPin<Error = EIN>,
    OUT: OutputPin<Error = EOUT>,
{
    type ReturnType = i32;
    type Error = ();

    fn read(&mut self) -> Result<Self::ReturnType, Self::Error> {
        match self.retrieve() {
            Ok(v) => Ok(v),
            Err(_) => Err(()),
        }
    }
}

pub trait Mapping
where
    Self: Copy + Into<i64>,
{
    fn map_to_i16(&self, min: Self, max: Self) -> i16 {
        let min = min.into();
        let max = max.into();
        let value = (*self).into().clamp(min, max);
        let range_in = max - min;
        let range_out = i16::MAX as i64 - i16::MIN as i64;

        if range_in != 0 {
            let scaled = ((value - min) * range_out / range_in) + i16::MIN as i64;
            scaled as i16
        } else {
            i16::MIN
        }
    }
}

impl<T> Mapping for T where T: Copy + Into<i64> {}

pub trait AnalogRead<Pin> {
    type ReturnType;

    fn read(&mut self, pin: &mut Pin) -> Self::ReturnType;
}

#[cfg(target_arch = "arm")]
impl<Pin, T> AnalogRead<Pin> for Adc<'_, T>
where
    Pin: embassy_stm32::adc::AdcChannel<T>,
    T: embassy_stm32::adc::Instance,
{
    type ReturnType = u16;

    fn read(&mut self, pin: &mut Pin) -> Self::ReturnType {
        self.blocking_read(pin)
    }
}

#[cfg(test)]
mod test_mapping {
    use crate::Mapping;
    use rstest::rstest;

    #[rstest]
    #[case(100, 0, 100, i16::MAX)]
    #[case(50, 0, 100, -1)]
    #[case(0, 0, 100, i16::MIN)]
    #[case(200, 100, 200, i16::MAX)]
    #[case(150, 100, 200, -1)]
    #[case(100, 100, 200, i16::MIN)]
    fn when_value_inside_the_input_range(
        #[case] value: u16,
        #[case] minimum: u16,
        #[case] maximum: u16,
        #[case] expected: i16,
    ) {
        // When
        let result = value.map_to_i16(minimum, maximum);

        // Then
        assert_eq!(result, expected);
    }

    #[rstest]
    #[case(101, 50, 100, i16::MAX)]
    #[case(49, 50, 100, i16::MIN)]
    fn when_value_outside_the_input_range(
        #[case] value: u16,
        #[case] minimum: u16,
        #[case] maximum: u16,
        #[case] expected: i16,
    ) {
        // When
        let result = value.map_to_i16(minimum, maximum);

        // Then
        assert_eq!(result, expected);
    }

    #[test]
    fn when_range_is_zero_size() {
        // Given
        let value: u16 = 100;
        let minimum: u16 = 100;
        let maximum: u16 = 100;

        // When
        let result = value.map_to_i16(minimum, maximum);

        // Then
        assert_eq!(result, i16::MIN);
    }

    #[rstest]
    #[case(100, 0, 100, i16::MAX)]
    #[case(50, 0, 100, -1)]
    #[case(0, 0, 100, i16::MIN)]
    #[case(101, 50, 100, i16::MAX)]
    #[case(49, 50, 100, i16::MIN)]
    fn when_value_is_i32(
        #[case] value: i32,
        #[case] minimum: i32,
        #[case] maximum: i32,
        #[case] expected: i16,
    ) {
        // When
        let result = value.map_to_i16(minimum, maximum);

        // Then
        assert_eq!(result, expected);
    }
}
