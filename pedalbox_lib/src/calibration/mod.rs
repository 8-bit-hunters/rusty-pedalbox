use bytemuck::{Pod, Zeroable};
use core::ops::{Add, AddAssign, Div, Sub, SubAssign};
use core::sync::atomic::{AtomicI32, AtomicU16, Ordering};

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
    type Atomic: 'static;

    fn zero() -> Self;
    fn one() -> Self;

    fn saturating_sub(self, rhs: Self) -> Self;
    fn saturating_add(self, rhs: Self) -> Self;
    fn store_in(self, cell: &Self::Atomic, order: Ordering);
}

impl Int for u16 {
    type Atomic = AtomicU16;

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

    fn store_in(self, cell: &Self::Atomic, order: Ordering) {
        cell.store(self, order);
    }
}

impl Int for i32 {
    type Atomic = AtomicI32;

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

    fn store_in(self, cell: &Self::Atomic, order: Ordering) {
        cell.store(self, order);
    }
}

#[repr(C)]
#[derive(Copy, Clone, Zeroable)]
pub struct StoredRange<T>
where
    T: Int,
{
    pub min: T,
    pub max: T,
}

unsafe impl Pod for StoredRange<u16> {}
