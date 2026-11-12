#![allow(clippy::missing_safety_doc)]
#![allow(clippy::manual_is_multiple_of)]

#[derive(Clone, Debug)]
pub enum Error {
    LayoutErr(core::alloc::LayoutError),
    AllocErr(core::alloc::AllocError),
    SizeOverflowErr,
}

impl std::convert::From<core::alloc::LayoutError> for Error {
    fn from(layout_error: core::alloc::LayoutError) -> Error {
        Error::LayoutErr(layout_error)
    }
}

impl std::convert::From<core::alloc::AllocError> for Error {
    fn from(alloc_error: core::alloc::AllocError) -> Error {
        Error::AllocErr(alloc_error)
    }
}

pub mod linear_alloc;
pub use linear_alloc::{LinearAlloc, LinearAllocError};

/// An array that is always 2-byte alligned
#[repr(align(2))]
pub struct Align2<T, const N: usize>(pub [T; N]);

/// An array that is always 4-byte alligned
#[repr(align(4))]
pub struct Align4<T, const N: usize>(pub [T; N]);

/// An array that is always 8-byte alligned
#[repr(align(8))]
pub struct Align8<T, const N: usize>(pub [T; N]);

/// An array that is always 16-byte alligned
#[repr(align(16))]
pub struct Align16<T, const N: usize>(pub [T; N]);

/// An array that is always 32-byte alligned
#[repr(align(32))]
pub struct Align32<T, const N: usize>(pub [T; N]);

/// An array that is always 64-byte alligned
#[repr(align(64))]
pub struct Align64<T, const N: usize>(pub [T; N]);

/// An array that is always 128-byte alligned
#[repr(align(128))]
pub struct Align128<T, const N: usize>(pub [T; N]);

/// An array that is always 256-byte alligned
#[repr(align(256))]
pub struct Align256<T, const N: usize>(pub [T; N]);

/// An array that is always 512-byte alligned
#[repr(align(512))]
pub struct Align512<T, const N: usize>(pub [T; N]);

/// An array that is always 1024-byte alligned
#[repr(align(1024))]
pub struct Align1024<T, const N: usize>(pub [T; N]);
