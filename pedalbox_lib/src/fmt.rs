#![allow(unused)]

#[macro_export]
macro_rules! assert {
    ($($x:tt)*) => {
        {
            #[cfg(feature = "defmt")]
            ::defmt::assert!($($x)*);
            #[cfg(not(feature = "defmt"))]
            ::core::assert!($($x)*);
        }
    };
}

#[macro_export]
macro_rules! assert_eq {
    ($($x:tt)*) => {
        {
            #[cfg(feature = "defmt")]
            ::defmt::assert_eq!($($x)*);
            #[cfg(not(feature = "defmt"))]
            ::core::assert_eq!($($x)*);
        }
    };
}

#[macro_export]
macro_rules! assert_ne {
    ($($x:tt)*) => {
        {
            #[cfg(feature = "defmt")]
            ::defmt::assert_ne!($($x)*);
            #[cfg(not(feature = "defmt"))]
            ::core::assert_ne!($($x)*);
        }
    };
}

#[macro_export]
macro_rules! debug_assert {
    ($($x:tt)*) => {
        {
            #[cfg(feature = "defmt")]
            ::defmt::debug_assert!($($x)*);
            #[cfg(not(feature = "defmt"))]
            ::core::debug_assert!($($x)*);
        }
    };
}

#[macro_export]
macro_rules! debug_assert_eq {
    ($($x:tt)*) => {
        {
            #[cfg(feature = "defmt")]
            ::defmt::debug_assert_eq!($($x)*);
            #[cfg(not(feature = "defmt"))]
            ::core::debug_assert_eq!($($x)*);
        }
    };
}

#[macro_export]
macro_rules! debug_assert_ne {
    ($($x:tt)*) => {
        {
            #[cfg(feature = "defmt")]
            ::defmt::debug_assert_ne!($($x)*);
            #[cfg(not(feature = "defmt"))]
            ::core::debug_assert_ne!($($x)*);
        }
    };
}

#[macro_export]
macro_rules! todo {
    ($($x:tt)*) => {
        {
            #[cfg(feature = "defmt")]
            ::defmt::todo!($($x)*);
            #[cfg(not(feature = "defmt"))]
            ::core::todo!($($x)*);
        }
    };
}

#[macro_export]
macro_rules! unreachable {
    ($($x:tt)*) => {
        {
            #[cfg(feature = "defmt")]
            ::defmt::unreachable!($($x)*);
            #[cfg(not(feature = "defmt"))]
            ::core::unreachable!($($x)*);
        }
    };
}

#[macro_export]
macro_rules! panic {
    ($($x:tt)*) => {
        {
            #[cfg(feature = "defmt")]
            ::defmt::panic!($($x)*);
            #[cfg(not(feature = "defmt"))]
            ::core::panic!($($x)*);
        }
    };
}

#[macro_export]
macro_rules! trace {
    ($s:literal $(, $x:expr)* $(,)?) => {
        {
            #[cfg(feature = "defmt")]
            ::defmt::trace!($s $(, $x)*);
        }
    };
}

#[macro_export]
macro_rules! debug {
    ($s:literal $(, $x:expr)* $(,)?) => {
        {
            #[cfg(feature = "defmt")]
            ::defmt::debug!($s $(, $x)*);
        }
    };
}

#[macro_export]
macro_rules! info {
    ($s:literal $(, $x:expr)* $(,)?) => {
        {
            #[cfg(feature = "defmt")]
            ::defmt::info!($s $(, $x)*);
        }
    };
}

#[macro_export]
macro_rules! _warn {
    ($s:literal $(, $x:expr)* $(,)?) => {
        {
            #[cfg(feature = "defmt")]
            ::defmt::warn!($s $(, $x)*);
        }
    };
}

#[macro_export]
macro_rules! error {
    ($s:literal $(, $x:expr)* $(,)?) => {
        {
            #[cfg(feature = "defmt")]
            ::defmt::error!($s $(, $x)*);
        }
    };
}

#[macro_export]
macro_rules! unwrap {
    ($($x:tt)*) => {
        {
            #[cfg(feature = "defmt")]
            ::defmt::unwrap!($($x)*);
            #[cfg(not(feature = "defmt"))]
            ::core::assert!($($x)*.is_ok()); // Simple mock for unwrap
        }
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

#[cfg(feature = "defmt")]
impl defmt::Format for Bytes<'_> {
    fn format(&self, fmt: defmt::Formatter) {
        defmt::write!(fmt, "{:02x}", self.0)
    }
}

#[cfg(feature = "defmt")]
pub use defmt::Format;
#[cfg(feature = "defmt")]
pub use defmt::Formatter;

#[cfg(not(feature = "defmt"))]
pub trait Format {}
#[cfg(not(feature = "defmt"))]
impl<T> Format for T {}
#[cfg(not(feature = "defmt"))]
pub struct Formatter;

#[cfg(not(feature = "defmt"))]
pub mod defmt {
    #[macro_export]
    macro_rules! mock_write {
        ($($t:tt)*) => {};
    }
    pub use mock_write as write;
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
