//! Access to LLVM Address Sanitizer callbacks
#![allow(non_camel_case_types)]

use core::ffi::c_int;
use core::ffi::c_void;
use core::ptr::NonNull;

/// Reminder that these addresses are `volatile` in C
type c_volatile_void = c_void;

// Found under compiler-rt/include/sanitizer/asan_interface.h
unsafe extern "C" {
    /// Marks memory region [addr, addr+size) as unaddressable.
    ///
    /// This memory must be previously allocated by the user program. Accessing
    /// addresses in this region from instrumented code is forbidden until
    /// this region is unpoisoned. This function is not guaranteed to poison
    /// the whole region - it may poison only subregion of [addr, addr+size) due
    /// to ASan alignment restrictions.
    /// Method is NOT thread-safe in the sense that no two threads can
    /// (un)poison memory in the same memory region simultaneously.
    unsafe fn __asan_poison_memory_region(addr: *const c_volatile_void, size: usize);

    /// Marks memory region [addr, addr+size) as addressable.
    ///
    /// This memory must be previously allocated by the user program. Accessing
    /// addresses in this region is allowed until this region is poisoned again.
    /// This function may unpoison a superregion of [addr, addr+size) due to
    /// ASan alignment restrictions.
    /// Method is NOT thread-safe in the sense that no two threads can
    /// (un)poison memory in the same memory region simultaneously.
    unsafe fn __asan_unpoison_memory_region(addr: *const c_volatile_void, size: usize);

    /// Checks if an address is poisoned.
    ///
    /// Returns 1 if <c><i>addr</i></c> is poisoned (that is, 1-byte read/write
    /// access to this address would result in an error report from ASan).
    /// Otherwise returns 0.
    ///
    /// \param addr Address to check.
    ///
    /// \retval 1 Address is poisoned.
    /// \retval 0 Address is not poisoned.
    unsafe fn __asan_address_is_poisoned(addr: *const c_volatile_void) -> c_int;

    /// Checks if a region is poisoned.
    ///
    /// If at least one byte in <c>[beg, beg+size)</c> is poisoned, returns the
    /// address of the first such byte. Otherwise returns 0.
    ///
    /// \param beg Start of memory region.
    /// \param size Start of memory region.
    /// \returns Address of first poisoned byte.
    unsafe fn __asan_region_is_poisoned(beg: *mut c_void, size: usize) -> *mut c_void;
}

/// Checks if an address is poisoned.
///
/// When `--cfg asan` is not enabled, always returns `false`
pub unsafe fn asan_address_is_poisoned(addr: NonNull<u8>) -> bool {
    if cfg!(asan) {
        unsafe { __asan_address_is_poisoned(addr.as_ptr() as _) != 0 }
    } else {
        false
    }
}

/// Checks if a region is poisoned.
///
/// If at least one byte in <c>[beg, beg+size)</c> is poisoned, returns the
/// address of the first such byte. Otherwise returns 0.
///
/// When `--cfg asan` is not enabled, always returns `None`.
pub unsafe fn asan_region_is_poisoned(region: NonNull<[u8]>) -> Option<NonNull<u8>> {
    if cfg!(asan) {
        unsafe {
            let ptr: *mut c_void = __asan_region_is_poisoned(region.as_ptr() as _, region.len());
            NonNull::new(ptr.cast())
        }
    } else {
        None
    }
}

/// Marks memory region [addr, addr+size) as unaddressable.
///
/// When `--cfg asan` is not enabled, this is a no-opt.
pub unsafe fn asan_poison_memory_region(region: NonNull<[u8]>) {
    if cfg!(asan) {
        unsafe {
            __asan_poison_memory_region(region.cast().as_ptr(), region.len());
        }
    }
}

/// Marks memory region [addr, addr+size) as addressable.
///
/// When `--cfg asan` is not enabled, this is a no-opt.
pub unsafe fn asan_unpoison_memory_region(region: NonNull<[u8]>) {
    if cfg!(asan) {
        unsafe {
            __asan_unpoison_memory_region(region.cast().as_ptr(), region.len());
        }
    }
}
