
use std::{
    alloc,
    result,
    ptr::NonNull,
};

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
/// # #![feature(allocator_api)]
/// # use std::alloc::*;
/// # use std::ptr;
/// # use alloc_utils::linear_alloc::LinearAlloc;
/// #
/// // Force the allocator to start on an 8-byte aligned boundary.
/// #[repr(align(8))] struct Buffer { buf: [u8; 24] }
/// let mut buf = Buffer { buf: [0u8; 24] };
///
/// let mut allocator = LinearAlloc::new(&mut buf.buf);
///
/// // The Allocator API is predominately unsafe.
/// unsafe {
///     // Allocate extremely small blocks.
///     let _ = allocator.alloc_one::<u8>().unwrap();
///     assert_eq!(allocator.bytes_in_use(), 1);
///
///     // Allocations are still aligned, and can "waste" space.
///     // u16 is 2-byte aligned, so we "waste" a byte.
///     let _ = allocator.alloc_one::<u16>().unwrap();
///     assert_eq!(allocator.bytes_in_use(), 4);
///
///     // Save spots in the stack.
///     let marker_at_4 = allocator.get_marker();
///
///     // Allocating arrays.
///     let _ = allocator.alloc_array::<u32>(2).unwrap();
///     assert_eq!(allocator.bytes_in_use(), 12);
///
///     let ptr = allocator.alloc_one::<u64>().unwrap();
///     assert_eq!(allocator.bytes_in_use(), 24);
///
///     // Deallocating blocks from the top actually frees them.
///     allocator.dealloc_one::<u64>(ptr);
///     assert_eq!(allocator.bytes_in_use(), 16);
///
///     // High water mark to see how bad it got.
///     assert_eq!(allocator.high_water_mark(), 24);
///
///     // Ooms fail gracefully.
///     let res = allocator.alloc_array::<u64>(6);
///     assert_eq!(res, Err(AllocErr));
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
    buf:  &'a [u8],
    // The current top of the stack as an index into buf.
    top:  usize,
    // The high water mark of the allocator, as an index into buf.
    high: usize,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Marker(usize);

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum LinearAllocError {
    // There was an attempt to reset the stack to a marker which is not valid.
    InvalidMarker,
}

type LinearAllocResult<T> = result::Result<T, LinearAllocError>;

impl <'a> LinearAlloc<'a> {

    /// Create a new linear allocator with a backing buffer.
    pub fn new(buf: &mut [u8]) -> LinearAlloc {
        LinearAlloc {
            buf,
            top:  0,
            high: 0,
        }
    }

    /// Resets the stack completely.
    ///
    /// This is unsafe because it marks all memory from this allocator as "free",
    /// even if there are still objects using this memory.
    /// It is the responsibility of the caller to ensure that this doesn't happen.
    pub unsafe fn reset(&mut self) {
        self.top = 0;
    }

    /// Resets the stack to a specified location.
    ///
    /// This is unsafe because it marks all memory from this allocator as "free",
    /// even if there are still objects using this memory.
    /// It is the responsibility of the caller to ensure that this doesn't happen.
    pub unsafe fn reset_to(&mut self, marker: Marker) -> LinearAllocResult<()> {
        if marker.0 < self.buf.len() &&
           marker.0 < self.top           // Don't reset "up".
        {
            self.top = marker.0;
            Ok(())
        } else {
            Err(LinearAllocError::InvalidMarker)
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

    /// Gets the length of the backing buffer.
    ///
    /// This is the largest number that `bytes_in_use` can ever return.
    pub fn capacity(&self) -> usize {
        self.buf.len()
    }

    /// Gets the "high water mark" of bytes that have been in use by this
    /// allocator at any one time, since its creation.
    ///
    /// This is not reset with calls to `reset()` or `reset_to()`.
    pub fn high_water_mark(&self) -> usize {
        self.high
    }

    /// Gets immutable access to the underlaying buffer.
    ///
    /// This can be used to peek at the buffer even with the allocator in use,
    /// since construction of the allocator involves a mutable borrow that lives
    /// as long as the allocator does.
    pub fn buf(&self) -> &[u32] {
        unsafe {
            use std::slice;
            slice::from_raw_parts(self.buf.as_ptr() as *const u32,
                                  self.buf.len() / 4)
        }
    }

    // Gets the index into self.buf at which the given pointer begins.
    fn get_block_idx(&self, ptr: NonNull<u8>) -> usize {
        ptr.as_ptr() as usize - self.buf.as_ptr() as usize
    }

}

unsafe impl <'a> alloc::Alloc for LinearAlloc<'a> {

    fn usable_size(&self, layout: &alloc::Layout) -> (usize, usize) {
        // Our allocations are tight, and do not include any excess.
        // This also sets the guarantees for `layout.size()` in other calls.
        // The caller is responsible for giving us a correct size.
        // This lets us walk back from the top of the stack and free allocations
        // if they are on top when `dealloc` is called, without saving metadata.
        (layout.size(), layout.size())
    }

    unsafe fn alloc(&mut self, layout: alloc::Layout)
        -> result::Result<NonNull<u8>, alloc::AllocErr>
    {
        if self.top == self.buf.len() {
            return Err(alloc::AllocErr);
        }

        let buf_base   = &self.buf[0]        as *const u8 as usize;
        let block_base = &self.buf[self.top] as *const u8 as usize;
        let block_base = block_base + (block_base % layout.align());

        // This is the pointer that the caller will receive, if we have room.
        // We got it by indexing into self.buf, so we know it can't be null.
        let ptr = NonNull::new_unchecked(block_base as *mut u8);

        let block_base_idx = block_base - buf_base;
        match (block_base_idx, block_base_idx.checked_add(layout.size())) {
            (block, Some(new_top)) if (block   <  self.buf.len() &&
                                       new_top <= self.buf.len()) =>
            {
                // Our allocated block is in bounds, and so is the new top!
                // Everything is good, so let's save our changes and return
                // the new pointer.
                self.top = new_top;
                self.high = self.high.max(self.top);
                Ok(ptr)
            },
            _ => {
                // We do not have enough space to satisfy this allocation.
                Err(alloc::AllocErr)
            },
        }
    }

    unsafe fn dealloc(&mut self, ptr: NonNull<u8>, layout: alloc::Layout) {
        // Because we return tight bounds for calls to `usable_size()`,
        // we can assume that this `layout` struct is exactly the size of our
        // block.

        // If our block is at the top of the stack, we can free it.
        let block_idx = self.get_block_idx(ptr);
        if block_idx + layout.size() == self.top {
            self.top = block_idx;
        }
        // Anything else... and we can't.
    }

    unsafe fn grow_in_place(&mut self,
                            ptr:      NonNull<u8>,
                            layout:   alloc::Layout,
                            new_size: usize)
        -> result::Result<(), alloc::CannotReallocInPlace>
    {
        let block_idx  = self.get_block_idx(ptr);
        let block_size;

        // We assert on these to catch errors quickly, but we do not guard
        // against them because they are *caller* errors.

        // The spec for `Alloc::grow_in_place` guarantees:
        //    1) ptr must be currently allocated via this allocator,
        assert!(block_idx < self.buf.len(),
                "Pointer is not from this allocator.");
        assert!(block_idx < self.top,
                "Pointer has already been freed, or is invalid.");
        // This is guaranteed not to underflow now.
        block_size = self.top - block_idx;
        //    2) layout must fit the ptr;
        //       note the new_size argument need not fit it
        assert_eq!(ptr.as_ptr() as usize % layout.align(), 0,
                   "Pointer does not fit layout.");
        assert!(self.usable_size(&layout).0 <= block_size,
                "The blocks size is too small?");
        //    3) new_size must not be greater than layout.size()
        //       (and must be greater than zero),
        assert!(new_size >= layout.size(),
                "Attempting to \"grow\" an allocation smaller.");
        assert_ne!(new_size, 0,
                "Attempting to \"grow\" an allocation to zero size.");

        let space_left = self.capacity() - self.bytes_in_use();
        // We need at this much additional room in order to grow in place.
        let block_growth = new_size - layout.size();
        if space_left < block_growth {
            return Err(alloc::CannotReallocInPlace);
        }

        // This wasn't the last block allocated, so we can't grow it in place.
        // Note: This test does not account for padding due to the alignment of
        //       a previous allocation that has since been freed.
        if block_idx + layout.size() != self.top {
            return Err(alloc::CannotReallocInPlace);
        }

        // Now we know that
        //      1) self.buf has enough room, and
        //      2) The block in question is at the top of the stack
        // So we can go ahead and bump self.top and call it success.
        self.top += block_growth;
        Ok(())
    }

    // ----- These may be useful to implement later. ----------------------------

    unsafe fn shrink_in_place(&mut self,
                              _ptr:      NonNull<u8>,
                              _layout:   alloc::Layout,
                              _new_size: usize)
        -> result::Result<(), alloc::CannotReallocInPlace>
    {
        Err(alloc::CannotReallocInPlace)
    }

}

#[cfg(test)]
mod t {

    use super::*;
    use std::{
        alloc::Alloc,
        mem,
    };

    // Used to tag whether something should be Ok(..) or Err(..), but without
    // caring about the values.
    #[derive(Copy, Clone, Debug, PartialEq)]
    enum R {
        Ok,
        Err,
    }

    impl R {

        fn from_result<O, E>(result: &result::Result<O, E>) -> R {
            if result.is_ok() {
                R::Ok
            } else {
                R::Err
            }
        }
    }

    #[test]
    fn check_simple_alloc() {
        let mut buf = [0u8; 2 * mem::size_of::<u32>()];

        // The pointers we expect to be valid are saved here, and used at the
        // end of the function.
        let ptrs: [NonNull<u8>; 2];

        // NLL cannot come quickly enough.
        // Force alloc to drop before we check our pointers at the end, because
        // alloc &muts buf, and we need to read buf to check the tests.
        // Some day, this can just use a mem::forget() call instead of scoping.
        {
            let mut alloc = LinearAlloc::new(&mut buf);

            let layout = alloc::Layout::new::<u32>();
            // This *should* be knowable at compile time, but Rust isn't there yet.
            assert_eq!(2 * layout.size(), alloc.capacity());

            // We expect two allocations to work, and then two to fail.
            // Failure should *not* abort the test!
            // unsafe due to calls to alloc::Alloc::alloc().
            unsafe {
                let allocs = [
                    alloc.alloc(layout),
                    alloc.alloc(layout),

                    alloc.alloc(layout),
                    alloc.alloc(layout),
                ];

                let expected_tags = [
                    R::Ok,
                    R::Ok,
                    R::Err,
                    R::Err,
                ];

                let actual_tags = [
                    R::from_result(&allocs[0]),
                    R::from_result(&allocs[1]),
                    R::from_result(&allocs[2]),
                    R::from_result(&allocs[3]),
                ];

                assert_eq!(actual_tags, expected_tags);

                ptrs = [
                    allocs[0].clone().unwrap(),
                    allocs[1].clone().unwrap(),
                ];
            }

            assert_ne!(NonNull::dangling(), ptrs[0]);
            assert_ne!(ptrs[0], ptrs[1]);
        }

        // Unsafe due to dereferencing pointers.
        unsafe {
            let a: &mut u32 = (ptrs[0].as_ptr() as *mut u32).as_mut().unwrap();
            *a = 23;
            assert_eq!(*a as u8, buf[0]);
            *a = 45;
            assert_eq!(*a as u8, buf[0]);

            let b: &mut u32 = (ptrs[1].as_ptr() as *mut u32).as_mut().unwrap();
            *b = 23;
            assert_eq!(*b as u8, buf[4]);
            *b = 45;
            assert_eq!(*b as u8, buf[4]);
        }
    }

    #[test]
    fn check_in_place_realloc() {
        let mut buf = [0u8; 3*8];
        let mut alloc = LinearAlloc::new(&mut buf);

        // Unsafe because of calls to alloc
        unsafe {
            let layout     = alloc::Layout::new::<[u8; 8]>();
            let p_first  = alloc.alloc(layout).expect("Couldn't alloc [0, 8]");
            let p_second = alloc.alloc(layout).expect("Couldn't alloc [8, 16]");

            let new_layout = alloc::Layout::new::<[u8; 16]>();
            alloc.grow_in_place(p_second, layout, 16)
                .expect("Couldn't grow in place from [8, 16] to [8, 24]");
            alloc.dealloc(p_second, new_layout);

            alloc.grow_in_place(p_first, layout, 16)
                .expect("Couldn't grow in place from [0, 8] to [0, 16]");
            alloc.dealloc(p_first, new_layout);
        }

        assert_eq!(alloc.bytes_in_use(), 0);
    }

}
