//! Datetime field accessors that work under the limited API (abi3 / abi3t).
//!
//! `PyO3`'s native [`PyDateAccess`], [`PyTimeAccess`], and [`PyDeltaAccess`]
//! traits are unavailable under the stable/limited API, which is what this
//! crate builds against for every interpreter except free-threaded 3.14t.
//! This module provides drop-in replacements that read the fields through
//! Python attribute access, used unconditionally so there is a single code
//! path.

use pyo3::intern;
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
                self.getattr(intern!(self.py(), "year"))
                    .and_then(|v| v.extract())
                    .expect("date.year")
            }
            fn get_month(&self) -> u8 {
                self.getattr(intern!(self.py(), "month"))
                    .and_then(|v| v.extract())
                    .expect("date.month")
            }
            fn get_day(&self) -> u8 {
                self.getattr(intern!(self.py(), "day"))
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
                self.getattr(intern!(self.py(), "hour"))
                    .and_then(|v| v.extract())
                    .expect("time.hour")
            }
            fn get_minute(&self) -> u8 {
                self.getattr(intern!(self.py(), "minute"))
                    .and_then(|v| v.extract())
                    .expect("time.minute")
            }
            fn get_second(&self) -> u8 {
                self.getattr(intern!(self.py(), "second"))
                    .and_then(|v| v.extract())
                    .expect("time.second")
            }
            fn get_microsecond(&self) -> u32 {
                self.getattr(intern!(self.py(), "microsecond"))
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
        self.getattr(intern!(self.py(), "days"))
            .and_then(|v| v.extract())
            .expect("timedelta.days")
    }
    fn get_seconds(&self) -> i32 {
        self.getattr(intern!(self.py(), "seconds"))
            .and_then(|v| v.extract())
            .expect("timedelta.seconds")
    }
}
