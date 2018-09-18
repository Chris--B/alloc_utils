#![feature(allocator_api)]
#![feature(core_intrinsics)]

#![deny(warnings)]

#[cfg(test)]
#[macro_use]
extern crate pretty_assertions;

macro_rules! debug_assert2 {
    ($pred:expr $(, $arg:tt)*) => {
        if cfg!(debug_assertions) {
            assert!($pred, $($arg)*);
        }
        #[allow(unused_unsafe)]
        unsafe {
            ::std::intrinsics::assume($pred);
        }
    }
}

#[derive(Clone, Debug)]
pub enum Error {
    LayoutErr(std::alloc::LayoutErr),
    AllocErr(std::alloc::AllocErr),
    SizeOverflowErr,
}

impl std::convert::From<std::alloc::LayoutErr> for Error {
    fn from(layout_error: std::alloc::LayoutErr) -> Error {
        Error::LayoutErr(layout_error)
    }
}

impl std::convert::From<std::alloc::AllocErr> for Error {
    fn from(alloc_error: std::alloc::AllocErr) -> Error {
        Error::AllocErr(alloc_error)
    }
}

pub mod linear_alloc;
pub mod raw_vec;
pub mod vec2;
