use std::{
    alloc,
    mem,
    ptr::NonNull,
    result,
};

// TODO: Failure crate
type VecResult<T> = result::Result<T, alloc::AllocErr>;

pub struct RawVec<'v, T> {
    // We store a pointer to the allocator instead of a reference to get around
    // mutability restrictions.
    // We cannot have the Vec exercise unilateral control over the allocator,
    // as we expect
    //  (1) other collections to use the same allocator, and
    //  (2) callers to interact with the allocator while the Vec does too.
    // We do still have lifetime guarantees, however.
    alloc: NonNull<alloc::Alloc + 'v>,
    ptr:   NonNull<T>, // Pointer to Ts
    cap:   usize,      // How many Ts we have space for.
}

impl <'v, T> RawVec<'v, T> {
    pub fn ptr(&self) -> *mut T {
        self.ptr.as_ptr()
    }

    pub fn capacity(&self) -> usize {
        self.cap
    }

    pub fn new(alloc: &mut (dyn alloc::Alloc + 'v)) -> Self {
        assert!(mem::size_of::<T>() != 0, "Zero Sized Types are not supported");
        RawVec {
            alloc: NonNull::new(alloc).unwrap(),
            ptr:   NonNull::dangling(),
            cap:   0,
        }
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
        // There's lots of room for error here, so let's call it all unsafe.
        unsafe {
            let new_cap: usize;
            let new_ptr: NonNull<T>;
            let layout:  alloc::Layout;

            if self.cap == 0 {
                new_cap = 1;
                layout  = alloc::Layout::array::<T>(new_cap).unwrap();
                new_ptr = self.alloc.as_mut().alloc(layout)?.cast();
            } else {
                // layout must refer to the *existing* allocation.
                layout  = self.alloc_layout();
                new_cap = 2 * self.cap;

                if let Ok(_) = self.alloc.as_mut().grow_in_place(self.ptr.cast(),
                                                                 layout,
                                                                 2 * layout.size())
                {
                    new_ptr = self.ptr;
                } else {
                    new_ptr = self.alloc.as_mut().realloc(self.ptr.cast(),
                                                          layout,
                                                          2 * layout.size())?.cast();
                }
            }

            self.cap = new_cap;
            self.ptr = new_ptr;
        }
        Ok(())
    }

}

impl <'v, T> Drop for RawVec<'v, T> {
    fn drop(&mut self) {
        if self.cap != 0 {
            unsafe {
                let layout = self.alloc_layout();
                self.alloc.as_mut().dealloc(self.ptr.cast(), layout);
            }
        }
    }
}

