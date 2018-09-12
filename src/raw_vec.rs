use std::{
    alloc,
    mem,
    ptr::NonNull,
    result,
};

use Error;
type VecResult<T> = result::Result<T, Error>;

pub struct RawVec<'v, T> {
    // We store a pointer to the allocator instead of a reference to get around
    // mutability restrictions.
    // We cannot have the Vec exercise unilateral control over the allocator,
    // as we expect
    //  (1) other collections to use the same allocator, and
    //  (2) callers to interact with the allocator while the Vec does too.
    // We do still have lifetime guarantees, however.
    alloc: NonNull<dyn alloc::Alloc + 'v>,
    ptr:   NonNull<T>, // Pointer to Ts
    cap:   usize,      // How many Ts we have space for.
}

impl <'v, T> RawVec<'v, T> {
    /// Create a new buffer. Does not allocate.
    pub fn new(alloc: &mut (dyn alloc::Alloc + 'v)) -> Self {
        assert!(mem::size_of::<T>() != 0, "Zero Sized Types are not supported");
        RawVec {
            alloc: NonNull::new(alloc).unwrap(),
            ptr:   NonNull::dangling(),
            cap:   0,
        }
    }

    /// Get the type erased Allocator that the Vec is using.
    pub fn alloc(&mut self) -> &mut dyn alloc::Alloc {
        unsafe { self.alloc.as_mut() }
    }

    /// Get the pointer to the buffer.
    pub fn ptr(&self) -> *mut T {
        self.ptr.as_ptr()
    }

    /// Get the number of Ts that the buffer has space for.
    pub fn capacity(&self) -> usize {
        self.cap
    }

    /// Create and allocate a new buffer.
    pub fn with_capacity(alloc: &mut (dyn alloc::Alloc + 'v),
                         capacity: usize)
        -> VecResult<Self>
    {
        assert!(mem::size_of::<T>() != 0, "Zero Sized Types are not supported");
        let mut raw_vec = RawVec {
            alloc: NonNull::new(alloc).unwrap(),
            ptr:   NonNull::dangling(),
            cap:   0,
        };
        raw_vec.reserve(capacity)?;
        Ok(raw_vec)
    }

    /// Get the Layout for the current allocation. This is suitable to pass to
    /// `alloc::Alloc` methods.
    pub fn alloc_layout(&self) -> alloc::Layout {
        // I'm not entirely sure how this could fail.
        alloc::Layout::array::<T>(self.cap).unwrap()
    }

    /// Each call to `grow` *doubles* the size of the allocation, which is
    /// initially space for a single T.
    pub fn grow(&mut self) -> VecResult<()> {
        if self.cap == 0 {
            self.reserve(1)
        } else {
            let cap = self.capacity();
            // Reserve an *additional* `cap` spaces
            self.reserve(cap)
        }
    }

    /// Increase the reserved space to hold at least `additional` more `T`s.
    pub fn reserve(&mut self, additional: usize) -> VecResult<()> {
        let new_cap: usize;
        let new_ptr: NonNull<T>;
        let layout:  alloc::Layout;

        // This is unsafe because of our calls to `*alloc` methods.
        // We'd use the `*alloc_array()` methods, but those aren't working
        // on a Trait object. (Why?)
        unsafe {
            // The first allocation is special - it goes through `Alloc::alloc`.
            if self.cap == 0 {
                new_cap = 1;
                layout  = alloc::Layout::array::<T>(additional).unwrap();
                new_ptr = self.alloc().alloc(layout)?.cast();
            // Otherwise, it can go through `Alloc::realloc`
            } else {
                // This layout must refer to the *existing* allocation.
                layout  = self.alloc_layout();
                new_cap = self.cap
                              .checked_add(additional)
                              .ok_or(Error::SizeOverflowErr)?;
                let new_layout = alloc::Layout::array::<T>(new_cap).unwrap();
                let new_size = new_layout.size();

                let ptr = self.ptr.cast();
                if let Ok(()) = self.alloc().grow_in_place(ptr, layout, new_size)
                {
                    // We resized our block in place and don't need to update.
                    // This is just here to make sure `new_ptr` is initialized.
                    new_ptr = self.ptr;
                } else {
                    // We cannot grow in place, so get a new block
                    // (and free the old one.)
                    new_ptr = self.alloc()
                                  .realloc(ptr, layout, new_size)?
                                  .cast();
                }
            }
        }

        self.cap = new_cap;
        self.ptr = new_ptr;

        Ok(())
    }
}

impl <'v, T> Drop for RawVec<'v, T> {
    fn drop(&mut self) {
        if self.cap != 0 {
            unsafe {
                let layout = self.alloc_layout();
                let ptr = self.ptr.cast();
                self.alloc().dealloc(ptr, layout);
            }
        }
    }
}
