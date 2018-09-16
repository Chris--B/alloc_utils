use std::{
    alloc,
    iter,
    marker,
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
/// # Examples
/// ```rust
/// # use alloc_utils::vec2::Vec;
/// #
/// let mut v = Vec::with_system_alloc();
/// v.extend_from_slice(&[1, 2, 3, 4, 5]);
/// assert_eq!(v.as_slice(), &[1, 2, 3, 4, 5]);
///
/// let mut drain = v.drain();
/// assert_eq!(drain.next(), Some(1));
/// assert_eq!(drain.next(), Some(2));
/// assert_eq!(drain.next(), Some(3));
/// assert_eq!(drain.next(), Some(4));
/// assert_eq!(drain.next(), Some(5));
/// ```
pub struct Vec<'v, T: 'v> {
    buf:   RawVec<'v, T>, // Resizeable memory buffer.
    len:   usize,         // Count of Ts stored.
}

impl <'v, T> Vec<'v, T> {
    /// Construct a new Vec
    pub fn new(alloc: &mut (dyn alloc::Alloc + 'v)) -> Self {
        Vec {
            buf: RawVec::new(alloc),
            len: 0,
        }
    }

    /// Construct a new Vec using the system allocator
    pub fn with_system_alloc() -> Self {
        Vec {
            buf: RawVec::with_system_alloc(),
            len: 0,
        }
    }

    /// Returns a pointer to the array of  elements.
    pub fn ptr(&self) -> *mut T {
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

    /// Move `elem` into the Vec, returning any allocation errors.
    ///
    /// # Examples
    /// ```rust
    /// # use alloc_utils::{vec2::Vec, Error};
    /// let mut v = Vec::with_system_alloc();
    ///
    /// match v.push(2) {
    ///     // Inserstion worked: Either no allocation happened, or it worked.
    ///     Ok(()) => {},
    ///     // If (re)allocation fails...
    ///     Err(err) => match err {
    ///         // Operations with `alloc::Alloc` and `alloc::Layout`
    ///         // can generate a `alloc::LayoutErr` error.
    ///         Error::LayoutErr(layout_err) => {
    ///             println!("layout error: {:?}", layout_err);
    ///         },
    ///         // Allocation errors propgate from `alloc::Alloc` as
    ///         // `alloc::AllocErr`.
    ///         Error::AllocErr(alloc_err) => {
    ///             println!("alloc error: {:?}", alloc_err);
    ///         },
    ///         // If layout or size calculations overflow, the resize fails.
    ///         Error::SizeOverflowErr => {
    ///             println!("Size overflowed trying to allocate");
    ///         },
    ///     }
    /// }
    ///
    /// assert_eq!(v.as_slice(), &[2]);
    /// ```
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

    /// Creates a draining iterator that removes elements from the Vec, and then
    /// yields them.
    pub fn drain(&mut self) -> Drain<T> {
        unsafe {
            let iter = RawValIter::new(&self);
            self.len = 0;
            Drain {
                _vec: marker::PhantomData,
                iter: iter,
            }
        }
    }
}

impl <'v, T> Vec<'v, T>
    where T: Clone
{
    /// Append items to the vector until `push` fails, or `iter` is exhausted.
    ///
    /// # Examples
    /// ```rust
    /// # use alloc_utils::vec2::Vec;
    /// #
    /// let mut v = Vec::<u32>::with_system_alloc();
    ///
    /// v.extend_from_slice(&[1, 2, 3, 4]).unwrap();
    /// assert_eq!(v.as_slice(), &[1, 2, 3, 4]);
    /// ```
    pub fn extend_from_slice(&mut self, slice: &[T]) -> VecResult<()>
    {
        for elem in slice {
            self.push(elem.clone())?;
        }
        Ok(())
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
        unsafe {
            // We need to use ptr::read to move the buf out, since it's not Copy
            // and Vec implements Drop (and so we can't destructure it)
            let buf  = ptr::read(&self.buf);
            let iter = RawValIter::new(&self);
            mem::forget(self);

            IntoIter {
                _buf: buf,
                iter: iter
            }
        }
    }
}

// ----- RawValIter & Traits ----------------------------------------------------

// Raw iterator base
pub struct RawValIter<T> {
    start:  *const T, // The next item in the iterator
    end:    *const T, // The next_back item in the iterator
}

impl <T> RawValIter<T> {
    // This is unsafe because it has no associated lifetimes.
    unsafe fn new(slice: &[T]) -> Self {
        RawValIter {
            start: slice.as_ptr(),
            end:   slice.as_ptr().offset(slice.len() as isize),
        }
    }
}

impl <T> iter::Iterator for RawValIter<T> {
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

impl <T> iter::DoubleEndedIterator for RawValIter<T> {
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

impl <T> iter::ExactSizeIterator for RawValIter<T> {}

impl <T> iter::FusedIterator for RawValIter<T> {}

// ----- IntoIter & Traits ------------------------------------------------------

// See `Vec::into_iter()`
pub struct IntoIter<'v, T: 'v> {
    _buf: RawVec<'v, T>, // This is unused; we just need it to live.
    iter: RawValIter<T>,
}

impl <'v, T> Drop for IntoIter<'v, T> {
    fn drop(&mut self) {
        // Drop all remaining items
        for _ in &mut *self {}
    }
}

impl <'v, T> iter::Iterator for IntoIter<'v, T> {
    type Item = T;

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }

    fn next(&mut self) -> Option<T> {
        self.iter.next()
    }
}

impl <'v, T> iter::DoubleEndedIterator for IntoIter<'v, T> {
    fn next_back(&mut self) -> Option<T> {
        self.iter.next_back()
    }
}

impl <'v, T> iter::ExactSizeIterator for IntoIter<'v, T> {}

impl <'v, T> iter::FusedIterator for IntoIter<'v, T> {}

// ----- Drain & Traits ---------------------------------------------------------

// See `Vec::drain()`
pub struct Drain<'a, 'v: 'a, T: 'v> {
    _vec: marker::PhantomData<&'a mut Vec<'v, T>>,
    iter: RawValIter<T>,
}

impl <'a, 'v, T> iter::Iterator for Drain<'a, 'v, T> {
    type Item = T;

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }

    fn next(&mut self) -> Option<T> {
        self.iter.next()
    }
}

impl <'a, 'v, T> iter::DoubleEndedIterator for Drain<'a, 'v, T> {
    fn next_back(&mut self) -> Option<T> {
        self.iter.next_back()
    }
}

impl <'a, 'v, T> iter::ExactSizeIterator for Drain<'a, 'v, T> {}

impl <'a, 'v, T> iter::FusedIterator for Drain<'a, 'v, T> {}

impl <'a, 'v, T> Drop for Drain<'a, 'v, T> {
    fn drop(&mut self) {
        // Drop all remaining items
        for _ in &mut *self {}
    }
}

// ----- Tests ------------------------------------------------------------------

#[cfg(test)]
mod t {
    use super::*;
    use linear_alloc::LinearAlloc;

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
        let mut alloc = LinearAlloc::new(&mut buf);
        let mut v = Vec::<u32>::new(&mut alloc);
        v.push(1).expect("v.push(1) failed.");

        assert_eq!(v.as_slice(), &[1]);
    }

    #[test]
    fn check_many_pushes_all_work() {
        let mut buf = [0u8; 43]; // Room for 10 u32s, and extra space.
        let mut alloc = LinearAlloc::new(&mut buf);
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
        let mut alloc = LinearAlloc::new(&mut buf);
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
        let mut alloc = LinearAlloc::new(&mut buf);

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
        let mut alloc = LinearAlloc::new(&mut buf);
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
