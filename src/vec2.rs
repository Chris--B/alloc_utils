use std::{
    alloc,
    iter,
    mem,
    ops,
    ptr::{
        self,
        NonNull,
    },
    result,
    slice,
};

// TODO: Failure crate
type VecResult<T> = result::Result<T, alloc::AllocErr>;

/// A grow-able and shrink-able dynamically sized array.
///
/// It differs from `std::vec::Vec` by storing its own allocator instead of
/// using the global or system allocators.
pub struct Vec<'v, T> {
    // We store a pointer to the allocator instead of a reference to get around
    // mutability restrictions.
    // We cannot have the Vec exercise unilateral control over the allocator,
    // as we expect
    //  (1) other collections to use the same allocator, and
    //  (2) callers to interact with the allocator while the Vec does too.
    // We do still have lifetime guarantees, however.
    alloc: NonNull<alloc::Alloc + 'v>,
    ptr:   NonNull<T>, // Pointer to Ts
    cap:   usize,      // How many Ts we can hold without growing.
    len:   usize,      // How many Ts we have initialized.
}

pub struct IntoIter<'v, T> {
    buf:    NonNull<T>,
    alloc:  NonNull<alloc::Alloc + 'v>,
    layout: alloc::Layout,
    start:  *const T,
    end:    *const T,
}

impl <'v, T> Vec<'v, T> {

    /// Construct a new Vec
    pub fn new(alloc: &mut (dyn alloc::Alloc + 'v)) -> Self {
        Vec {
            alloc: NonNull::new(alloc).unwrap(),
            ptr:   NonNull::dangling(),
            cap:   0,
            len:   0,
        }
    }

    /// Move `elem` into the Vec, returning any allocation errors.
    pub fn push(&mut self, elem: T) -> VecResult<()> {
        if self.len == self.cap {
            self.grow()?;
        }
        unsafe {
            ptr::write(self.ptr.as_ptr().offset(self.len as isize), elem);
        }
        self.len += 1;
        Ok(())
    }

    /// Move the elem at the end of the Vec out, returning it.
    ///
    /// Returns `None` if the Vec is empty.
    pub fn pop(&mut self) -> Option<T> {
        if self.len == 0 {
            None
        } else {
            Some(unsafe {
                self.len -= 1;
                ptr::read(self.ptr.as_ptr().offset(self.len as isize))
            })
        }
    }

    /// Move `elem` into the Vec at index `index`, moving any elements if needed
    /// and returning any allocation errors.
    pub fn insert(&mut self, index: usize, elem: T) -> VecResult<()> {
        assert!(index <= self.len);
        if self.cap == self.len {
            self.grow()?;
        }

        unsafe {
            if index < self.len {
                ptr::copy(self.ptr.as_ptr().offset(index as isize),
                          self.ptr.as_ptr().offset(index as isize + 1),
                          self.len - index);
            }
            ptr::write(self.ptr.as_ptr().offset(index as isize), elem);
            self.len += 1;
        }

        Ok(())
    }

    /// Moves `elem` out of the Vec and shifts all elements over to fill its spot.
    pub fn remove(&mut self, index: usize) -> T {
        assert!(index < self.len);
        let corpse;
        unsafe {
            self.len -= 1;
            corpse = ptr::read(self.ptr.as_ptr().offset(index as isize));
            ptr::copy(self.ptr.as_ptr().offset(index as isize + 1),
                      self.ptr.as_ptr().offset(index as isize),
                      self.len - index);
        }
        corpse
    }

    /// Returns a slice of the Vec's elements
    pub fn as_slice(&self) -> &[T] {
        self
    }

    /// Returns the mutable slice of the Vec's elements
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        self
    }

    // Increase the allocated backing buffer of the Vec.
    //
    // Each call to `grow` doubles the size of the allocation, which is
    // initially space for a single T.
    fn grow(&mut self) -> VecResult<()> {
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

    // Get the Layout for the current allocation. This is suitable to pass to
    // `alloc::Alloc` methods.
    fn alloc_layout(&self) -> alloc::Layout {
        // This should never fail.
        alloc::Layout::array::<T>(self.cap).unwrap()
    }

}

// ----- Vec Traits -------------------------------------------------------------

impl <'v, T> Drop for Vec<'v, T> {

    fn drop(&mut self) {
        if self.cap != 0 {
            // We must call each destructor. If T doesn't impl Drop, this loop
            // is optimized out.
            while let Some(_) = self.pop() {};

            unsafe {
                let layout = self.alloc_layout();
                self.alloc.as_mut().dealloc(self.ptr.cast(), layout);
            }
        }
    }

}

impl <'v, T> ops::Deref for Vec<'v, T> {

    type Target = [T];

    fn deref(&self) -> &[T] {
        unsafe {
            slice::from_raw_parts(self.ptr.as_ptr(), self.len)
        }
    }

}

impl <'v, T> ops::DerefMut for Vec<'v, T> {

    fn deref_mut(&mut self) -> &mut [T] {
        unsafe {
            slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len)
        }
    }

}

impl <'v, T> iter::IntoIterator for Vec<'v, T> {
    type Item = T;
    type IntoIter = ::vec2::IntoIter<'v, T>;

    fn into_iter(self) -> Self::IntoIter {
        // We're the vector now.
        let alloc  = self.alloc;
        let ptr    = self.ptr;
        let len    = self.len;
        let layout = self.alloc_layout();
        mem::forget(self);

        unsafe {
            IntoIter {
                buf: ptr,
                alloc: alloc,
                layout,
                start: ptr.as_ptr(),
                end:   ptr.as_ptr().offset(len as isize),
            }
        }
    }
}

// ----- IntoIter Traits --------------------------------------------------------

impl <'v, T> iter::Iterator for IntoIter<'v, T> {

    type Item = T;

    fn size_hint(&self) -> (usize, Option<usize>) {
        let start = self.start as usize;
        let end   = self.end as usize;
        let len   = (end - start) / mem::size_of::<T>();
        (len, Some(len))
    }

    fn next(&mut self) -> Option<T> {
        if self.start == self.end {
            None
        } else {
            unsafe {
                let item = ptr::read(self.start);
                self.start = self.start.offset(1);
                Some(item)
            }
        }
    }

}

impl <'v, T> iter::DoubleEndedIterator for IntoIter<'v, T> {

    fn next_back(&mut self) -> Option<T> {
        if self.start == self.end {
            None
        } else {
            unsafe {
                self.end = self.end.offset(-1);
                Some(ptr::read(self.end))
            }
        }
    }

}

impl <'v, T> iter::ExactSizeIterator for IntoIter<'v, T> {
    // The default implementation is enough.
}

impl <'v, T> iter::FusedIterator for IntoIter<'v, T> {
    // The default implementation is enough.
}

impl <'v, T> Drop for IntoIter<'v, T> {

    fn drop(&mut self) {
        // Drop all remaining items
        for _ in &mut *self {}

        // And deallocate
        unsafe {
            let layout = self.layout;
            self.alloc.as_mut().dealloc(self.buf.cast(), layout);
        }
    }
}

// ----- Tests ------------------------------------------------------------------


#[cfg(test)]
mod t {

    use super::*;
    use stack_alloc::StackAlloc;

    use std::{
        cell,
        mem,
    };

    // A helper type that increments shared data when it is dropped.
    struct DropMe<'a> {
        data: &'a cell::RefCell<u32>,
    }

    impl <'a> Drop for DropMe<'a> {

        fn drop(&mut self) {
            *self.data.borrow_mut() += 1;
        }

    }


    #[test]
    fn check_one_push_works() {
        let mut buf = [0u8; 43]; // Room for 10 u32s, and extra space.
        let mut alloc = StackAlloc::new(&mut buf);
        let mut v = Vec::<u32>::new(&mut alloc);
        v.push(1).expect("v.push(1) failed.");

        assert_eq!(v.as_slice(), &[1]);
    }

    #[test]
    fn check_many_pushes_all_work() {
        let mut buf = [0u8; 43]; // Room for 10 u32s, and extra space.
        let mut alloc = StackAlloc::new(&mut buf);
        let mut v = Vec::<u32>::new(&mut alloc);
        v.push(1).expect("v.push(1) failed.");
        v.push(2).expect("v.push(2) failed.");
        v.push(3).expect("v.push(3) failed.");
        v.push(4).expect("v.push(4) failed.");

        assert!(alloc.high_water_mark() > 0);
        assert_eq!(v.as_slice(), &[1, 2, 3, 4]);
    }

    #[test]
    fn check_two_vectors_one_alloc() {
        let mut buf = [0u8; 56];
        let mut alloc = StackAlloc::new(&mut buf);
        let mut v = Vec::<u32>::new(&mut alloc);
        let mut w = Vec::<u32>::new(&mut alloc);

        println!("[]   {:?}",  alloc.buf());

        v.push(1).expect("v.push(1) failed.");
        println!("[1]  {:?}",  alloc.buf());
        w.push(11).expect("w.push(11) failed.");
        println!("[11] {:?}",  alloc.buf());
        println!("");

        v.push(2).expect("v.push(2) failed.");
        println!("[2]  {:?}",  alloc.buf());
        w.push(22).expect("w.push(22) failed.");
        println!("[22] {:?}",  alloc.buf());
        println!("");

        v.push(3).expect("v.push(3) failed.");
        println!("[3]  {:?}",  alloc.buf());
        w.push(33).expect("w.push(33) failed.");
        println!("[33] {:?}",  alloc.buf());
        println!("");

        v.push(4).expect("v.push(4) failed.");
        println!("[4]  {:?}",  alloc.buf());
        w.push(44).expect("w.push(44) failed.");
        println!("[44] {:?}",  alloc.buf());
        println!("");

        assert_eq!(&[1, 2, 3, 4],     v.as_slice());
        assert_eq!(&[11, 22, 33, 44], w.as_slice());
    }

    #[test]
    fn check_drop_called() {
        let mut buf = [0u8; 128];
        let mut alloc = StackAlloc::new(&mut buf);

        let data = &cell::RefCell::new(0);

        let mut v = Vec::<DropMe>::new(&mut alloc);
        v.push(DropMe { data }).expect("push(..) failed.");
        v.push(DropMe { data }).expect("push(..) failed.");
        v.push(DropMe { data }).expect("push(..) failed.");
        v.push(DropMe { data }).expect("push(..) failed.");
        v.push(DropMe { data }).expect("push(..) failed.");
        v.push(DropMe { data }).expect("push(..) failed.");
        v.push(DropMe { data }).expect("push(..) failed.");
        v.push(DropMe { data }).expect("push(..) failed.");
        v.push(DropMe { data }).expect("push(..) failed.");

        assert_eq!(*data.borrow(), 0);
        mem::drop(v);
        assert_eq!(*data.borrow(), 9);
    }

    #[test]
    fn check_insert_and_remove() {
        let mut buf = [0u8; 128];
        let mut alloc = StackAlloc::new(&mut buf);
        let mut v = Vec::<u64>::new(&mut alloc);

        v.push(2*1).expect("push(2*1) failed.");
        v.push(2*2).expect("push(2*2) failed.");
        v.push(2*3).expect("push(2*3) failed.");
        v.push(2*4).expect("push(2*4) failed.");
        v.push(2*5).expect("push(2*5) failed.");
        v.push(2*6).expect("push(2*6) failed.");
        v.push(2*7).expect("push(2*7) failed.");
        assert_eq!(&[2, 4, 6, 8, 10, 12, 14], v.as_slice());

        v.insert(3, 1001).expect("v.insert(3, 1001) failed.");
        assert_eq!(&[2, 4, 6, 1001, 8, 10, 12, 14], v.as_slice());

        let corpse = v.remove(4);
        assert_eq!(&[2, 4, 6, 1001, 10, 12, 14], v.as_slice());
        assert_eq!(corpse, 8);
    }

}
