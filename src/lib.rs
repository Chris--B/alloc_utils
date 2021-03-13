#![feature(allocator_api)]

// #![deny(warnings)]

#[cfg(test)]
#[macro_use]
extern crate pretty_assertions;

#[derive(Clone, Debug)]
pub enum Error {
    LayoutErr(std::alloc::LayoutError),
    AllocErr(std::alloc::AllocError),
    SizeOverflowErr,
}

impl std::convert::From<std::alloc::LayoutError> for Error {
    fn from(layout_error: std::alloc::LayoutError) -> Error {
        Error::LayoutErr(layout_error)
    }
}

impl std::convert::From<std::alloc::AllocError> for Error {
    fn from(alloc_error: std::alloc::AllocError) -> Error {
        Error::AllocErr(alloc_error)
    }
}

pub mod linear_alloc;
// pub mod raw_vec;
// pub mod vec2;
