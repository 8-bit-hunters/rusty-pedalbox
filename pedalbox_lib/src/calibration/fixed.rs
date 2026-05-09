use crate::calibration::{Int, Range, StoredRange};

#[derive(Debug, Default, Copy, Clone, PartialEq)]
pub struct FixedRange<T: Int> {
    min: T,
    max: T,
}

impl<T: Int> FixedRange<T> {
    pub fn min(self, value: T) -> FixedRange<T> {
        Self {
            min: value,
            max: self.max,
        }
    }

    pub fn max(self, value: T) -> FixedRange<T> {
        Self {
            min: self.min,
            max: value,
        }
    }
}

impl<T: Int> Range<T> for FixedRange<T> {
    fn get_min(&self) -> T {
        self.min
    }

    fn get_max(&self) -> T {
        self.max
    }

    fn to_stored(&self) -> StoredRange<T> {
        StoredRange {
            min: self.min,
            max: self.max,
        }
    }

    fn from(stored: StoredRange<T>) -> FixedRange<T> {
        Self {
            min: stored.min,
            max: stored.max,
        }
    }
}
