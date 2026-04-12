//! Datetime field accessors compatible with both the full CPython API and abi3.
//!
//! Under the full API, PyO3 provides [`PyDateAccess`], [`PyTimeAccess`], and
//! [`PyDeltaAccess`] traits with fast C-level field access.  These are
//! unavailable under the stable/limited API (abi3).
//!
//! This module re-exports those traits when available, and provides drop-in
//! replacements that use Python attribute access when they are not.

#[cfg(not(feature = "abi3"))]
pub use pyo3::types::{PyDateAccess, PyDeltaAccess, PyTimeAccess};

#[cfg(feature = "abi3")]
mod abi3 {
    use pyo3::prelude::*;
    use pyo3::types::{PyDate, PyDateTime, PyDelta, PyTime};

    pub trait PyDateAccess {
        fn get_year(&self) -> i32;
        fn get_month(&self) -> u8;
        fn get_day(&self) -> u8;
    }

    macro_rules! impl_date_access {
        ($ty:ty) => {
            impl PyDateAccess for Bound<'_, $ty> {
                fn get_year(&self) -> i32 {
                    self.getattr("year")
                        .and_then(|v| v.extract())
                        .expect("date.year")
                }
                fn get_month(&self) -> u8 {
                    self.getattr("month")
                        .and_then(|v| v.extract())
                        .expect("date.month")
                }
                fn get_day(&self) -> u8 {
                    self.getattr("day")
                        .and_then(|v| v.extract())
                        .expect("date.day")
                }
            }
        };
    }

    impl_date_access!(PyDate);
    impl_date_access!(PyDateTime);

    pub trait PyTimeAccess {
        fn get_hour(&self) -> u8;
        fn get_minute(&self) -> u8;
        fn get_second(&self) -> u8;
        fn get_microsecond(&self) -> u32;
    }

    macro_rules! impl_time_access {
        ($ty:ty) => {
            impl PyTimeAccess for Bound<'_, $ty> {
                fn get_hour(&self) -> u8 {
                    self.getattr("hour")
                        .and_then(|v| v.extract())
                        .expect("time.hour")
                }
                fn get_minute(&self) -> u8 {
                    self.getattr("minute")
                        .and_then(|v| v.extract())
                        .expect("time.minute")
                }
                fn get_second(&self) -> u8 {
                    self.getattr("second")
                        .and_then(|v| v.extract())
                        .expect("time.second")
                }
                fn get_microsecond(&self) -> u32 {
                    self.getattr("microsecond")
                        .and_then(|v| v.extract())
                        .expect("time.microsecond")
                }
            }
        };
    }

    impl_time_access!(PyTime);
    impl_time_access!(PyDateTime);

    pub trait PyDeltaAccess {
        fn get_days(&self) -> i32;
        fn get_seconds(&self) -> i32;
    }

    impl PyDeltaAccess for Bound<'_, PyDelta> {
        fn get_days(&self) -> i32 {
            self.getattr("days")
                .and_then(|v| v.extract())
                .expect("timedelta.days")
        }
        fn get_seconds(&self) -> i32 {
            self.getattr("seconds")
                .and_then(|v| v.extract())
                .expect("timedelta.seconds")
        }
    }
}

#[cfg(feature = "abi3")]
pub use abi3::*;
