//! A basic linear allocator

use core::alloc::Allocator;
use core::alloc::{AllocError, Layout};
use core::marker::PhantomData;
use core::ptr::NonNull;
use core::sync::atomic::{AtomicUsize, Ordering::*};

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
///     let _ = allocator.allocate(Layout::new::<u8>()).unwrap();
///     assert_eq!(allocator.bytes_in_use(), 1);
///
///     // Allocations are still aligned, and can "waste" space.
///     // u16 is 2-byte aligned, so we "waste" a byte.
///     let _ = allocator.allocate(Layout::new::<u16>()).unwrap();
///     assert_eq!(allocator.bytes_in_use(), 4);
///
///     // Save spots in the stack.
///     let marker_at_4 = allocator.get_marker();
///
///     // Allocating arrays.
///     let _ = allocator.allocate(Layout::array::<u32>(2).unwrap()).unwrap();
///     assert_eq!(allocator.bytes_in_use(), 12);
///
///     let ptr = allocator.allocate(Layout::new::<u64>()).unwrap();
///     assert_eq!(allocator.bytes_in_use(), 24);
///
/// #   // Deallocating blocks from the top actually frees them.
/// #   // allocator.deallocate(ptr.cast(), Layout::new::<u64>());
/// #   // assert_eq!(allocator.bytes_in_use(), 16);
/// #
///     // High water mark to see how much we peaked at!
///     assert_eq!(allocator.high_water_mark(), 24);
///
///     // Ooms fail gracefully.
///     let res = allocator.allocate(Layout::array::<u64>(6).unwrap());
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
    /// The buffer backing our allocations
    p_buf: NonNull<[u8]>,

    buf_ref: PhantomData<&'a [u8]>,

    // The top of the stack
    top: AtomicUsize,

    // The high water mark of the allocator
    high: AtomicUsize,
}

/// A marker that can be used to reset a [`LinearAlloc`] to a prior state
///
/// Get a [`Marker`] from [`LinearAlloc::get_marker()`]. Use [`LinearAlloc::reset_to()`] reset the allocator back to that state.
/// This will deallocate any allocation made after the [`Marker`] was created.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Marker(usize);

impl<'a> LinearAlloc<'a> {
    /// Create a new linear allocator with a backing buffer.
    pub fn new(buf: &'_ mut [u8]) -> LinearAlloc<'_> {
        {
            let p_buf = NonNull::new(buf as *mut [u8]).unwrap();
            LinearAlloc {
                p_buf,
                buf_ref: PhantomData,
                top: AtomicUsize::new(0),
                high: AtomicUsize::new(0),
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
        self.top.store(0, Relaxed);
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
        // Note: We'll only reset to a `marker` that's less than our `top`.
        // Since we're behind a `&mut` here, we expect exclusive access to our fields.
        let top = self.top.get_mut();
        if marker.0 < *top {
            *top = marker.0;
        }
    }

    /// Gets a marker that the stack can be reset to later.
    pub fn get_marker(&self) -> Marker {
        Marker(self.top.load(Relaxed))
    }

    /// Gets the number of bytes currently allocated.
    pub fn bytes_in_use(&self) -> usize {
        self.top.load(Relaxed)
    }

    /// Gets the base pointer to the entire buffer, without invalidating existing borrows.
    pub fn get_ptr(&self) -> NonNull<[u8]> {
        self.p_buf
    }

    /// Gets the length of the backing buffer.
    ///
    /// This is the largest number that `bytes_in_use` can ever return.
    pub fn capacity(&self) -> usize {
        self.p_buf.len()
    }

    /// Gets the "high water mark" of bytes that have been in use by this
    /// allocator at any one time, since its creation.
    ///
    /// This is not reset with calls to `reset()` or `reset_to()`.
    pub fn high_water_mark(&self) -> usize {
        self.high.load(Relaxed)
    }
}

unsafe impl Allocator for LinearAlloc<'_> {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        unsafe {
            let top = self.top.load(Relaxed);
            if top >= self.capacity() || !layout.align().is_power_of_two() {
                // The allocator has run out of space - we cannot handle this request
                return Err(AllocError);
            }

            // We attempt to grab a block that will fit our layout, but if someone beats us to it we need to retry
            // Note: Contention is where dragons live, we need to investigate this.

            // block_base is the address where our new block starts - build it from the top of the current stack.
            let mut block_base: NonNull<u8> = self.get_ptr().cast::<u8>().offset(top as isize);

            let align_fix = block_base.align_offset(layout.align());
            block_base = block_base.add(align_fix);

            let new_top = top + align_fix + layout.size();
            if new_top > self.capacity() {
                // This block would be too large to fit - we cannot handle this request
                return Err(AllocError);
            }

            // Nothing we read or write is dependent on this updated top, so we use Relaxed ordering.
            if self
                .top
                .compare_exchange(top, new_top, Relaxed, Relaxed)
                .is_ok()
            {
                // Our high watermark is just book keeping and nothing depends on it, so we use Relaxed ordering.
                self.high.fetch_max(new_top, Relaxed);

                return Ok(NonNull::slice_from_raw_parts(block_base, layout.size()));
            }

            // Contention! We should retry I guess?
            unimplemented!("contended case not handled")
        }
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        #[cfg(test)]
        {
            println!("[{:?}] dealloc {ptr:?} {layout:?}", self.p_buf);
        }

        // dealloc is a no-op. Use the `Marker` API to reset the allocator
        // TODO: Can we dealloc a block if it's at the top?
        let _ = (ptr, layout);
    }
}

#[cfg(test)]
mod t {
    use super::*;

    use crate::align::Align4;

    #[test]
    fn check_simple_allocate() {
        let mut buf = Align4([0u8; 2 * sizeof!(u32)]);
        let buf = &mut buf.0;
        let alloc = LinearAlloc::new(buf);

        let layout = Layout::new::<u32>();
        assert_eq!(2 * layout.size(), alloc.capacity());

        // We expect two allocations to work, and then two to fail.
        // Failure should *not* abort the test!
        // unsafe due to calls to alloc::Alloc::allocate().
        let ptr_0 = alloc.allocate(layout);
        assert_eq!(
            ptr_0.err(),
            None,
            "alloc 1/4 failed, but should have worked"
        );

        let ptr_1 = alloc.allocate(layout);
        assert_eq!(
            ptr_1.err(),
            None,
            "alloc 2/4 failed, but should have worked"
        );
        assert_eq!(
            alloc.allocate(layout).err(),
            Some(AllocError),
            "alloc 3/4 worked, but should have failed"
        );
        assert_eq!(
            alloc.allocate(layout).err(),
            Some(AllocError),
            "alloc 4/4 worked, but should have failed"
        );

        let ptrs: [NonNull<u32>; 2] = [ptr_0.unwrap().cast(), ptr_1.unwrap().cast()];

        assert_ne!(NonNull::dangling(), ptrs[0]);
        assert_ne!(NonNull::dangling(), ptrs[1]);
        assert_ne!(ptrs[0], ptrs[1]);

        unsafe {
            // If we construct this pointer through a new `&mut ` from `buf`, it invalidates
            // the existing pointers and is UB.
            // The sort of UB that never actually breaks, but does fail Miri.
            let p_buf: NonNull<[u8]> = alloc.get_ptr();
            assert!(p_buf.len() >= 4);
            let p_buf: NonNull<u32> = p_buf.cast();
            assert!(p_buf.is_aligned());

            ptrs[0].write(23);
            assert_eq!(ptrs[0].read(), p_buf.add(0).read());

            ptrs[0].write(45);
            assert_eq!(ptrs[0].read(), p_buf.add(0).read());

            ptrs[1].write(23);
            assert_eq!(ptrs[1].read(), p_buf.add(1).read());

            ptrs[1].write(45);
            assert_eq!(ptrs[1].read(), p_buf.add(1).read());
        }
    }

    #[test]
    fn check_marker_ordering() {
        let mut buf = Align4([0u8; 3 * 8]);
        let mut alloc = LinearAlloc::new(&mut buf.0);
        let layout = Layout::new::<u32>();

        let marker0 = alloc.get_marker();
        let _p0 = alloc.allocate(layout).unwrap();

        let marker1 = alloc.get_marker();
        let _p1 = alloc.allocate(layout).unwrap();

        let marker2 = alloc.get_marker();
        let _p2 = alloc.allocate(layout).unwrap();

        assert!(alloc.bytes_in_use() >= 3 * sizeof!(u32));

        assert_ne!(marker0, marker1);
        assert_ne!(marker1, marker2);
        assert_ne!(marker0, marker2);

        assert!(marker0 < marker1);
        assert!(marker1 < marker2);
        assert!(marker0 < marker2);

        // TODO: Generation/Seq tracking to catch invalidated Markers after an alloc.reset();
        unsafe {
            alloc.reset();
        }
    }

    #[test]
    fn check_many_pushes_work() {
        let mut buf = Align4([0_u8; 10 * sizeof!(i32)]);
        let alloc = LinearAlloc::new(&mut buf.0);
        let mut v = Vec::<u32, &LinearAlloc>::new_in(&alloc);

        v.push(1);
        v.push(2);
        v.push(3);
        v.push(4);

        println!("bytes_in_use={}", alloc.bytes_in_use());
        println!("high_water_mark={}", alloc.high_water_mark());
        assert!(alloc.high_water_mark() > 0);
        assert!(alloc.bytes_in_use() >= 4 * sizeof!(i32));
        assert_eq!(v.as_slice(), &[1, 2, 3, 4]);
    }

    #[test]
    fn check_many_pushes_all_work_and_reset_behaves_well() {
        let mut buf = Align4([0_u8; 10 * sizeof!(i32)]);
        let mut alloc = LinearAlloc::new(&mut buf.0);

        unsafe {
            for i in 0..2 {
                println!("[{i}] Resetting alloc");
                alloc.reset();
                let mut v = Vec::<u32, &LinearAlloc>::new_in(&alloc);

                v.push(1);
                v.push(2);
                v.push(3);
                v.push(4);

                println!("[{i}] bytes_in_use={}", alloc.bytes_in_use());
                println!("[{i}] high_water_mark={}", alloc.high_water_mark());
                assert!(alloc.high_water_mark() > 0);
                assert!(alloc.bytes_in_use() >= 4 * sizeof!(i32));
                assert_eq!(v.as_slice(), &[1, 2, 3, 4]);
            }
        }
    }

    #[test]
    fn check_two_vectors_one_alloc() {
        let mut buf = Align4([0u8; 30 * sizeof!(i32)]);
        let alloc = LinearAlloc::new(&mut buf.0);
        let mut v = Vec::<u32, &LinearAlloc>::new_in(&alloc);
        let mut w = Vec::<u32, &LinearAlloc>::new_in(&alloc);

        fn print_buf(a: &LinearAlloc) -> [i32; 14] {
            unsafe {
                let a: core::ptr::NonNull<u8> = a.get_ptr().cast();
                let a: *mut u8 = a.as_ptr();
                let mut b = [0_i32; 14];

                core::ptr::copy(a, b.as_mut_ptr() as *mut u8, b.len() * sizeof!(i32));

                b
            }
        }

        println!("[]   {:?}", print_buf(&alloc));

        v.push(1);
        println!("[1]  {:?}", print_buf(&alloc));
        w.push(11);
        println!("[11] {:?}", print_buf(&alloc));
        println!();

        v.push(2);
        println!("[2]  {:?}", print_buf(&alloc));
        w.push(22);
        println!("[22] {:?}", print_buf(&alloc));
        println!();

        v.push(3);
        println!("[3]  {:?}", print_buf(&alloc));
        w.push(33);
        println!("[33] {:?}", print_buf(&alloc));
        println!();

        v.push(4);
        println!("[4]  {:?}", print_buf(&alloc));
        w.push(44);
        println!("[44] {:?}", print_buf(&alloc));
        println!();

        v.push(5);
        println!("[5]  {:?}", print_buf(&alloc));
        w.push(55);
        println!("[55] {:?}", print_buf(&alloc));
        println!();

        assert_eq!(&[1, 2, 3, 4, 5], v.as_slice());
        assert_eq!(&[11, 22, 33, 44, 55], w.as_slice());
    }
}
