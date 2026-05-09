#![no_std]

#[cfg(test)]
extern crate alloc;

pub mod calibration;
pub mod fmt;
pub mod io_monitors;
#[cfg(feature = "stm32")]
pub mod platform_stm32;
pub mod storage;

pub mod prelude {
    pub use super::fmt::*;
    pub use super::{AnalogRead, Mapping};
    pub use crate::io_monitors::*;
}

pub trait LoadCell {
    type ReturnType;
    type Error;
    fn read(&mut self) -> Result<Self::ReturnType, Self::Error>;
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

#[cfg(feature = "stm32")]
impl<'d, Pin, T> AnalogRead<Pin> for embassy_stm32::adc::Adc<'d, T>
where
    Pin: embassy_stm32::adc::AdcChannel<T>,
    T: embassy_stm32::adc::Instance,
    <<T as embassy_stm32::adc::BasicInstance>::Regs as embassy_stm32::adc::BasicAdcRegs>::SampleTime: From<embassy_stm32::adc::SampleTime>,
{
    type ReturnType = u16;

    fn read(&mut self, pin: &mut Pin) -> Self::ReturnType {
        self.blocking_read(pin, embassy_stm32::adc::SampleTime::CYCLES3.into())
    }
}

#[cfg(feature = "stm32")]
impl<D, IN, OUT, EIN, EOUT> LoadCell for hx711::Hx711<D, IN, OUT>
where
    D: embedded_hal::blocking::delay::DelayUs<u32>,
    IN: embedded_hal::digital::v2::InputPin<Error = EIN>,
    OUT: embedded_hal::digital::v2::OutputPin<Error = EOUT>,
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
