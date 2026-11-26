#![allow(clippy::missing_safety_doc)]
#![allow(clippy::manual_is_multiple_of, clippy::collapsible_if)]

#[macro_export]
macro_rules! sizeof {
    ($t:ty) => {
        ::core::mem::size_of::<$t>()
    };
    ($e:expr) => {
        ::core::mem::size_of_val(&$e)
    }
}

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
pub use linear_alloc::LinearAlloc;

pub mod asan;

/// Wrapper types that are aligned to a specific value
pub mod align {
    /// A wapper that is always 2-byte aligned
    #[repr(align(2))]
    pub struct Align2<T>(pub T);

    /// A wapper that is always 4-byte aligned
    #[repr(align(4))]
    pub struct Align4<T>(pub T);

    /// A wapper that is always 8-byte aligned
    #[repr(align(8))]
    pub struct Align8<T>(pub T);

    /// A wapper that is always 16-byte aligned
    #[repr(align(16))]
    pub struct Align16<T>(pub T);

    /// A wapper that is always 32-byte aligned
    #[repr(align(32))]
    pub struct Align32<T>(pub T);

    /// A wapper that is always 64-byte aligned
    #[repr(align(64))]
    pub struct Align64<T>(pub T);

    /// A wapper that is always 128-byte aligned
    #[repr(align(128))]
    pub struct Align128<T>(pub T);

    /// A wapper that is always 256-byte aligned
    #[repr(align(256))]
    pub struct Align256<T>(pub T);

    /// A wapper that is always 512-byte aligned
    #[repr(align(512))]
    pub struct Align512<T>(pub T);

    /// A wapper that is always 1024-byte aligned
    #[repr(align(1024))]
    pub struct Align1024<T>(pub T);
}

#[cfg(test)]
mod test {
    #[test]
    fn check_sizeof() {
        assert_eq!(sizeof!(u32), core::mem::size_of::<u32>());
        assert_eq!(sizeof!(0_u32), core::mem::size_of_val(&(0_u32)));
    }
}
