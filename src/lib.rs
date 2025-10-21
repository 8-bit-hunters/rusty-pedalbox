#![cfg_attr(not(test), no_std)]

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

impl Mapping for u16 {}

impl Mapping for i32 {}

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
