use bytemuck::{Pod, Zeroable};
use core::ops::{Add, AddAssign, Div, Sub, SubAssign};

#[cfg(feature = "adaptive-calibration")]
pub mod adaptive;
pub mod fixed;

pub trait Range<T: Int> {
    fn get_min(&self) -> T;
    fn get_max(&self) -> T;
    fn update(&mut self, _value: T) {}
    fn to_stored(&self) -> StoredRange<T>;
    fn from(stored: StoredRange<T>) -> Self;
}

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

#[repr(C)]
#[derive(Copy, Clone, Zeroable)]
pub struct StoredRange<T: Int> {
    pub min: T,
    pub max: T,
}

unsafe impl Pod for StoredRange<u16> {}
