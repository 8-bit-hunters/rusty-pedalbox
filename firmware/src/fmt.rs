#![allow(unused)]

#[macro_export]
macro_rules! assert {
    ($($x:tt)*) => {
        {
            ::defmt::assert!($($x)*);
        }
    };
}

#[macro_export]
macro_rules! assert_eq {
    ($($x:tt)*) => {
        {
            ::defmt::assert_eq!($($x)*);
        }
    };
}

#[macro_export]
macro_rules! assert_ne {
    ($($x:tt)*) => {
        {
            ::defmt::assert_ne!($($x)*);
        }
    };
}

#[macro_export]
macro_rules! debug_assert {
    ($($x:tt)*) => {
        {
            ::defmt::debug_assert!($($x)*);
        }
    };
}

#[macro_export]
macro_rules! debug_assert_eq {
    ($($x:tt)*) => {
        {
            ::defmt::debug_assert_eq!($($x)*);
        }
    };
}

#[macro_export]
macro_rules! debug_assert_ne {
    ($($x:tt)*) => {
        {
            ::defmt::debug_assert_ne!($($x)*);
        }
    };
}

#[macro_export]
macro_rules! todo {
    ($($x:tt)*) => {
        {
            ::defmt::todo!($($x)*);
        }
    };
}

#[macro_export]
macro_rules! unreachable {
    ($($x:tt)*) => {
        ::defmt::unreachable!($($x)*)
    };
}

#[macro_export]
macro_rules! panic {
    ($($x:tt)*) => {
        {
            ::defmt::panic!($($x)*);
        }
    };
}

#[macro_export]
macro_rules! trace {
    ($s:literal $(, $x:expr)* $(,)?) => {
        {
            ::defmt::trace!($s $(, $x)*);
        }
    };
}

#[macro_export]
macro_rules! debug {
    ($s:literal $(, $x:expr)* $(,)?) => {
        {
            ::defmt::debug!($s $(, $x)*);
        }
    };
}

#[macro_export]
macro_rules! info {
    ($s:literal $(, $x:expr)* $(,)?) => {
        {
            ::defmt::info!($s $(, $x)*);
        }
    };
}

#[macro_export]
macro_rules! _warn {
    ($s:literal $(, $x:expr)* $(,)?) => {
        {
            ::defmt::warn!($s $(, $x)*);
        }
    };
}

#[macro_export]
macro_rules! error {
    ($s:literal $(, $x:expr)* $(,)?) => {
        {
            ::defmt::error!($s $(, $x)*);
        }
    };
}

#[macro_export]
macro_rules! unwrap {
    ($($x:tt)*) => {
        ::defmt::unwrap!($($x)*)
    };
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct NoneError;

pub trait Try {
    type Ok;
    type Error;
    fn into_result(self) -> Result<Self::Ok, Self::Error>;
}

impl<T> Try for Option<T> {
    type Ok = T;
    type Error = NoneError;

    #[inline]
    fn into_result(self) -> Result<T, NoneError> {
        self.ok_or(NoneError)
    }
}

impl<T, E> Try for Result<T, E> {
    type Ok = T;
    type Error = E;

    #[inline]
    fn into_result(self) -> Self {
        self
    }
}

pub(crate) struct Bytes<'a>(pub &'a [u8]);

impl defmt::Format for Bytes<'_> {
    fn format(&self, fmt: defmt::Formatter) {
        defmt::write!(fmt, "{:02x}", self.0)
    }
}

#[cfg(target_arch = "x86_64")]
pub mod defmt {
    pub trait Format {}

    impl<T> Format for T {}
}

pub use _warn as warn;
pub use assert;
pub use assert_eq;
pub use assert_ne;
pub use debug;
pub use debug_assert;
pub use debug_assert_eq;
pub use debug_assert_ne;
pub use error;
pub use info;
pub use panic;
pub use todo;
pub use trace;
pub use unreachable;
pub use unwrap;
