//! A basic linear allocator

use core::alloc::{AllocError, Layout};
use core::marker::PhantomData;
use core::ptr::NonNull;
use core::result;

/// A linear allocator which uses a supplied-slice as backing memory.
///
/// The user supplied slice can exist on the stack or heap, but it must outlive
/// the allocator.
/// Allocation requests are given exactly as much memory as they ask for, and
/// can only be reused after being `dealloc`ed if they were the latest allocation
/// made from this allocator.
///
/// (Note: If all allocations are `dealloc`ed in the exact,
/// opposite order in which they were `alloc`ed, then all allocations can be
/// reused for further memory requests.)
///
/// ```rust
/// # use std::alloc::*;
/// # use std::ptr;
/// # use alloc_utils::LinearAlloc;
/// # use alloc_utils::align::*;
/// #
/// // Force the allocator to start on an 8-byte aligned boundary.
/// let mut buf = Align8([0u8; 24]);
///
/// let mut allocator = LinearAlloc::new(&mut buf.0);
/// unsafe {
///     use core::alloc::Allocator;
///     // Allocate extremely small blocks.
///     let _ = allocator.alloc(Layout::new::<u8>()).unwrap();
///     assert_eq!(allocator.bytes_in_use(), 1);
///
///     // Allocations are still aligned, and can "waste" space.
///     // u16 is 2-byte aligned, so we "waste" a byte.
///     let _ = allocator.alloc(Layout::new::<u16>()).unwrap();
///     assert_eq!(allocator.bytes_in_use(), 4);
///
///     // Save spots in the stack.
///     let marker_at_4 = allocator.get_marker();
///
///     // Allocating arrays.
///     let _ = allocator.alloc(Layout::array::<u32>(2).unwrap()).unwrap();
///     assert_eq!(allocator.bytes_in_use(), 12);
///
///     let ptr = allocator.alloc(Layout::new::<u64>()).unwrap();
///     assert_eq!(allocator.bytes_in_use(), 24);
///
///     // Deallocating blocks from the top actually frees them.
///     allocator.dealloc
/// (ptr.cast(), Layout::new::<u64>());
///     assert_eq!(allocator.bytes_in_use(), 16);
///
///     // High water mark to see how bad it got.
///     assert_eq!(allocator.high_water_mark(), 24);
///
///     // Ooms fail gracefully.
///     let res = allocator.alloc(Layout::array::<u64>(6).unwrap());
///     assert_eq!(res, Err(AllocError));
///
///     // Restore saved locations.
///     allocator.reset_to(marker_at_4);
///     assert_eq!(allocator.bytes_in_use(), 4);
///
///     // Hard reset of all allocations.
///     allocator.reset();
///     assert_eq!(allocator.bytes_in_use(), 0);
/// }
/// ```
#[derive(Debug)]
pub struct LinearAlloc<'a> {
    // The buffer backing allocations
    start: *mut u8,
    end: *mut u8,

    buf_ref: PhantomData<&'a [u8]>,

    // The current top of the stack as an index into buf.
    top: usize,
    // The high water mark of the allocator, as an index into buf.
    high: usize,
}

/// A marker that can be used to reset a [`LinearAlloc`] to a prior state
///
/// Get a [`Marker`] from [`LinearAlloc::get_marker()`]. Use [`LinearAlloc::reset_to()`] reset the allocator back to that state.
/// This will deallocate any allocation made after the [`Marker`] was created.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Marker(usize);

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum LinearAllocError {
    /// There was an attempt to reset the stack to a marker which is not valid.
    InvalidMarker,

    CannotReallocInPlace,
}
use LinearAllocError::*;

type LinearAllocResult<T> = result::Result<T, LinearAllocError>;

impl<'a> LinearAlloc<'a> {
    /// Create a new linear allocator with a backing buffer.
    pub fn new(buf: &'_ mut [u8]) -> LinearAlloc<'_> {
        unsafe {
            let len = buf.len();
            let start = buf.as_mut_ptr();
            let end = start.add(len);
            LinearAlloc {
                start,
                end,
                buf_ref: PhantomData,
                top: 0,
                high: 0,
            }
        }
    }

    /// Resets the stack to its initial state, deallocating all memory allocated from it.
    ///
    /// # Safety
    /// This deallocates all memory allocated from this allocator.
    ///
    /// It is undefined behavior to do this while any memory allocated from this allocator is still in use.
    ///
    /// Calling this function invalidates existing [`Marker`] objects.
    pub unsafe fn reset(&mut self) {
        self.top = 0;
    }

    /// Resets the stack to a specified location, deallocating some memory allocated from it.
    ///
    /// # Safety
    /// This deallocates all memory allocated after `marker` was created.
    ///
    /// It is undefined behavior to do this while that memory is still in use.
    ///
    /// Calling this function invalidates [`Marker`] objects that compare [`core::cmp::Ordering::Greater`] to `marker`.
    pub unsafe fn reset_to(&mut self, marker: Marker) {
        // Don't reset "up" - anything invalid is ignored
        if marker.0 < self.top {
            self.top = marker.0;
        }
    }

    /// Gets a marker that the stack can be reset to later.
    pub fn get_marker(&self) -> Marker {
        Marker(self.top)
    }

    /// Gets the number of bytes currently allocated.
    pub fn bytes_in_use(&self) -> usize {
        self.top
    }

    /// Gets the base pointer to the entire buffer, without invalidating existing borrows.
    pub fn get_ptr(&self) -> *mut u8 {
        self.start
    }

    /// Gets the length of the backing buffer.
    ///
    /// This is the largest number that `bytes_in_use` can ever return.
    pub fn capacity(&self) -> usize {
        unsafe { self.end.byte_offset_from_unsigned(self.start) }
    }

    /// Gets the "high water mark" of bytes that have been in use by this
    /// allocator at any one time, since its creation.
    ///
    /// This is not reset with calls to `reset()` or `reset_to()`.
    pub fn high_water_mark(&self) -> usize {
        self.high
    }

    // Gets the index into self.buf at which the given pointer begins.
    fn get_block_idx(&self, ptr: NonNull<u8>) -> usize {
        unsafe {
            debug_assert!((self.start..self.end).contains(&(ptr.as_ptr())));
            ptr.as_ptr().offset_from_unsigned(self.start)
        }
    }

    // This function is carefully written! Be careful when making changes.
    // It has no direct panic calls in release builds.
    pub unsafe fn alloc(&mut self, layout: Layout) -> result::Result<NonNull<u8>, AllocError> {
        unsafe {
            if self.top >= self.capacity() {
                dbg!();
                return Err(AllocError);
            }

            // block_base is the address where our new block starts.
            // It needs to be computed from buf and top, but also adjusted for alignment
            let mut block_base = self.start.add(self.top);
            let align_fix = block_base.align_offset(layout.align());
            dbg!(align_fix);
            block_base = block_base.add(align_fix);

            // block_idx is the index into our backing buf where this block starts.
            let block_idx = self.top + align_fix;
            // chked_new_top is the overflow-proof end of the newly created block.
            let chked_new_top = block_idx.checked_add(layout.size());
            match dbg!(block_idx, chked_new_top) {
            (index, Some(new_top))
                // Verify that the block still *starts* in bounds.
                // Alignment may have adjusted the block out of our range,
                // in which case we cannot handle this request.
                if (index < self.capacity() &&
                // Verify that the block still *ends* in bounds.
                // It is OK for new_top to be exactly the same as the buffer length.
                // This is expected when filling the allocator perfectly.
                    new_top <= self.capacity()) =>
            {
                self.top  = new_top;
                self.high = self.high.max(self.top);
                Ok(NonNull::new_unchecked(block_base))
            },
            _ => {
                // We do not have enough space to satisfy this allocation.
                dbg!();
                Err(AllocError)
            },
        }
        }
    }

    pub unsafe fn dealloc(&mut self, ptr: NonNull<u8>, layout: Layout) {
        // Because we return tight bounds in alloc(),
        // we can assume that this `layout` struct is exactly the size of our
        // block.

        // If our block is at the top of the stack, we can free it.
        let block_idx = self.get_block_idx(ptr);
        if block_idx + layout.size() == self.top {
            self.top = block_idx;
        }
        // Anything else... and we can't.
    }

    pub unsafe fn grow_in_place(
        &mut self,
        ptr: NonNull<u8>,
        layout: Layout,
        new_size: usize,
    ) -> LinearAllocResult<()> {
        let block_idx = self.get_block_idx(ptr);

        // We assert on these to catch errors quickly, but we do not guard
        // against them because they are *caller* errors.

        // The spec for `Alloc::grow_in_place` guarantees:
        //    1) ptr must be currently allocated via this allocator,
        assert!(
            block_idx < self.capacity(),
            "Pointer is not from this allocator."
        );
        assert!(
            block_idx < self.top,
            "Pointer has already been freed, or is invalid."
        );
        // This is guaranteed not to underflow now.
        let block_size = self.top - block_idx;
        //    2) layout must fit the ptr;
        //       note the new_size argument need not fit it
        assert!(
            ptr.as_ptr() as usize % layout.align() == 0,
            "Pointer does not fit layout."
        );
        assert!(layout.size() <= block_size, "The blocks size is too small?");
        //    3) new_size must not be greater than layout.size()
        //       (and must be greater than zero),
        assert!(
            new_size >= layout.size(),
            "Attempting to \"grow\" an allocation smaller."
        );
        assert!(
            new_size != 0,
            "Attempting to \"grow\" an allocation to zero size."
        );

        let space_left = self.capacity() - self.bytes_in_use();
        // We need at this much additional room in order to grow in place.
        let block_growth = new_size - layout.size();
        if space_left < block_growth {
            return Err(CannotReallocInPlace);
        }

        // This wasn't the last block allocated, so we can't grow it in place.
        // Note: This test does not account for padding due to the alignment of
        //       a previous allocation that has since been freed.
        if block_idx + layout.size() != self.top {
            return Err(CannotReallocInPlace);
        }

        // Now we know that
        //      1) self.buf has enough room, and
        //      2) The block in question is at the top of the stack
        // So we can go ahead and bump self.top and call it success.
        self.top += block_growth;
        Ok(())
    }

    // ----- This may be useful to implement later. ----------------------------
    #[allow(unused)]
    unsafe fn shrink_in_place(
        &mut self,
        _ptr: NonNull<u8>,
        _layout: Layout,
        _new_size: usize,
    ) -> LinearAllocResult<()> {
        Err(CannotReallocInPlace)
    }
}

#[cfg(test)]
mod t {

    use super::*;

    use crate::align::Align4;

    #[test]
    fn check_simple_alloc() {
        let mut buf = Align4([0u8; 2 * core::mem::size_of::<u32>()]);
        let buf = &mut buf.0;
        let mut alloc = LinearAlloc::new(buf);

        let layout = Layout::new::<u32>();
        assert_eq!(2 * layout.size(), alloc.capacity());

        // We expect two allocations to work, and then two to fail.
        // Failure should *not* abort the test!
        // unsafe due to calls to alloc::Alloc::alloc().
        let ptrs: [NonNull<u8>; 2];
        unsafe {
            let ptr_0 = alloc.alloc(layout);
            assert_eq!(
                ptr_0.err(),
                None,
                "alloc 1/4 failed, but should have worked"
            );

            let ptr_1 = alloc.alloc(layout);
            assert_eq!(
                ptr_1.err(),
                None,
                "alloc 2/4 failed, but should have worked"
            );
            assert_eq!(
                alloc.alloc(layout).err(),
                Some(AllocError),
                "alloc 3/4 worked, but should have failed"
            );
            assert_eq!(
                alloc.alloc(layout).err(),
                Some(AllocError),
                "alloc 4/4 worked, but should have failed"
            );

            ptrs = [ptr_0.unwrap(), ptr_1.unwrap()];

            assert_ne!(NonNull::dangling(), ptrs[0]);
            assert_ne!(NonNull::dangling(), ptrs[1]);
            assert_ne!(ptrs[0], ptrs[1]);
        }

        unsafe {
            // If we construct this pointer through a new `&mut ` from `buf`, it invalidates
            // the existing pointers and is UB.
            // The sort of UB that never actually breaks, but does fail Miri.
            let p_buf = alloc.get_ptr();

            ptrs[0].write(23);
            assert_eq!(ptrs[0].read(), p_buf.add(0).read());

            ptrs[0].write(45);
            assert_eq!(ptrs[0].read(), p_buf.add(0).read());

            ptrs[1].write(23);
            assert_eq!(ptrs[1].read(), p_buf.add(4).read());

            ptrs[1].write(45);
            assert_eq!(ptrs[1].read(), p_buf.add(4).read());
        }
    }

    #[test]
    fn check_in_place_realloc() {
        let mut buf = [0u8; 3 * 8];
        let mut alloc = LinearAlloc::new(&mut buf);

        // Unsafe because of calls to alloc
        unsafe {
            let layout = Layout::new::<[u8; 8]>();
            let p_first = alloc.alloc(layout).expect("Couldn't alloc [0, 8]");
            let p_second = alloc.alloc(layout).expect("Couldn't alloc [8, 16]");

            let new_layout = Layout::new::<[u8; 16]>();
            alloc
                .grow_in_place(p_second, layout, 16)
                .expect("Couldn't grow in place from [8, 16] to [8, 24]");
            alloc.dealloc(p_second, new_layout);

            alloc
                .grow_in_place(p_first, layout, 16)
                .expect("Couldn't grow in place from [0, 8] to [0, 16]");
            alloc.dealloc(p_first, new_layout);
        }

        assert_eq!(alloc.bytes_in_use(), 0);
    }

    #[test]
    fn check_marker_ordering() {
        let mut buf = [0u8; 3 * 8];
        let mut alloc = LinearAlloc::new(&mut buf);
        let layout = Layout::new::<u32>();

        unsafe {
            let marker0 = alloc.get_marker();
            let _p0 = alloc.alloc(layout).unwrap();

            let marker1 = alloc.get_marker();
            let _p1 = alloc.alloc(layout).unwrap();

            let marker2 = alloc.get_marker();
            let _p2 = alloc.alloc(layout).unwrap();

            alloc.dealloc(_p2, layout);

            assert_ne!(marker0, marker1);
            assert_ne!(marker1, marker2);
            assert_ne!(marker0, marker2);

            assert!(marker0 < marker1);
            assert!(marker1 < marker2);
            assert!(marker0 < marker2);
        }

        // TODO: Generation/Seq tracking to catch invalidated Markers after an alloc.reset();
    }
}
