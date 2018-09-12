use std::{
    alloc,
    iter,
    mem,
    ops,
    ptr,
    result,
    slice,
};

use raw_vec::RawVec;
use Error;

// TODO: Failure crate
type VecResult<T> = result::Result<T, Error>;

// ----- Vec Impl ---------------------------------------------------------------


/// A continuous growable array type with a customizable memory allocator.
///
/// It differs from `std::vec::Vec` by storing its own allocator instead of
/// using the global or system allocators.
pub struct Vec<'v, T> {
    buf:   RawVec<'v, T>, // Resizeable memory buffer.
    len:   usize,         // Count of Ts stored.
}

impl <'v, T> Vec<'v, T> {
    fn ptr(&self) -> *mut T {
        self.buf.ptr()
    }

    /// The number of items that the Vec can hold before resizing.
    pub fn capacity(&self) -> usize {
        self.buf.capacity()
    }

    /// The number of items currently in the Vec.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Construct a new Vec
    pub fn new(alloc: &mut (dyn alloc::Alloc + 'v)) -> Self {
        Vec {
            buf: RawVec::new(alloc),
            len: 0,
        }
    }

    /// Move `elem` into the Vec, returning any allocation errors.
    pub fn push(&mut self, elem: T) -> VecResult<()> {
        if self.len == self.capacity() {
            self.buf.grow()?;
        }
        unsafe {
            ptr::write(self.ptr().offset(self.len as isize), elem);
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
                ptr::read(self.ptr().offset(self.len as isize))
            })
        }
    }

    /// Move `elem` into the Vec at index `index`, moving any elements if needed
    /// and returning any allocation errors.
    pub fn insert(&mut self, index: usize, elem: T) -> VecResult<()> {
        assert!(index <= self.len);
        if self.len == self.capacity() {
            self.buf.grow()?;
        }

        unsafe {
            if index < self.len {
                ptr::copy(self.ptr().offset(index as isize),
                          self.ptr().offset(index as isize + 1),
                          self.len - index);
            }
            ptr::write(self.ptr().offset(index as isize), elem);
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
            corpse = ptr::read(self.ptr().offset(index as isize));
            ptr::copy(self.ptr().offset(index as isize + 1),
                      self.ptr().offset(index as isize),
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
}

// ----- Vec Traits -------------------------------------------------------------

impl <'v, T> Drop for Vec<'v, T> {
    fn drop(&mut self) {
        if self.capacity() != 0 {
            // We must call each destructor.
            // If T doesn't impl Drop, this loop is optimized out.
            while let Some(_) = self.pop() {};
        }
    }
}

impl <'v, T> ops::Deref for Vec<'v, T> {
    type Target = [T];

    fn deref(&self) -> &[T] {
        unsafe {
            slice::from_raw_parts(self.ptr(), self.len)
        }
    }
}

impl <'v, T> ops::DerefMut for Vec<'v, T> {
    fn deref_mut(&mut self) -> &mut [T] {
        unsafe {
            slice::from_raw_parts_mut(self.ptr(), self.len)
        }
    }
}

impl <'v, T> iter::IntoIterator for Vec<'v, T> {
    type Item = T;
    type IntoIter = IntoIter<'v, T>;

    fn into_iter(self) -> Self::IntoIter {
        // We need to use ptr::read to move the buf out, since it's not Copy and
        // Vec implements Drop (and so we can't destructure it)
        let buf = unsafe { ptr::read(&self.buf) };
        let ptr = self.ptr();
        let len = self.len();
        mem::forget(self);

        unsafe {
            IntoIter {
                _buf:  buf,
                start: ptr,
                end:   ptr.offset(len as isize),
            }
        }
    }
}

// ----- IntoIter  --------------------------------------------------------------

pub struct IntoIter<'v, T> {
    _buf:   RawVec<'v, T>,  // The memory backing the items.
                            // This is held onto to drop, but is unused.
    start:  *const T,       // The next item in the iterator
    end:    *const T,       // The next_back item in the iterator
}

impl <'v, T> iter::Iterator for IntoIter<'v, T> {
    type Item = T;

    fn size_hint(&self) -> (usize, Option<usize>) {
        let start = self.start as usize;
        let end   = self.end   as usize;
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
