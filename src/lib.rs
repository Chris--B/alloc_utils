#![feature(allocator_api)]

#![deny(warnings)]

#[cfg(test)]
#[macro_use]
extern crate pretty_assertions;

use std::{
    alloc,
    ops,
    ptr::{
        self,
        NonNull,
    },
    result,
    slice,
};

pub mod stack_alloc;

// TODO: Failure crate
type VecResult<T> = result::Result<T, alloc::AllocErr>;

pub struct Vec<'v, T> {
    alloc: NonNull<alloc::Alloc + 'v>,
    ptr:   NonNull<T>,
    cap:   usize,
    len:   usize,
}

impl <'v, T> Vec<'v, T> {

    pub fn new(alloc: &mut (dyn alloc::Alloc + 'v)) -> Self {
        Vec {
            alloc: NonNull::new(alloc).unwrap(),
            ptr:   NonNull::dangling(),
            cap:   0,
            len:   0,
        }
    }

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

    fn grow(&mut self) -> VecResult<()> {
        // There's lots of room for error here, so let's call it all unsafe.
        unsafe {
            let new_cap: usize;
            let new_ptr: NonNull<T>;
            let layout: alloc::Layout;

            if self.cap == 0 {
                new_cap = 1;
                layout  = alloc::Layout::array::<T>(new_cap).unwrap();
                new_ptr = self.alloc.as_mut().alloc(layout)?.cast();
            } else {
                new_cap = 2 * self.cap;
                layout  = alloc::Layout::array::<T>(new_cap).unwrap();
                new_ptr = self.alloc.as_mut().realloc(self.ptr.cast(),
                                             layout,
                                             layout.size())?.cast();
            }

            self.cap = new_cap;
            self.ptr = new_ptr;
        }
        Ok(())
    }

}

impl <'v, T> Drop for Vec<'v, T> {

    fn drop(&mut self) {
        if self.cap != 0 {
            // We must call each destructor. If T doesn't impl Drop, this loop
            // is optimized out.
            while let Some(_) = self.pop() {};

            unsafe {
                let layout = alloc::Layout::array::<T>(self.cap).unwrap();
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

#[cfg(test)]
mod t {

    use super::*;
    use stack_alloc::StackAlloc;

    #[allow(unused_imports)]
    use std::{
        mem,
        ops::Deref,
    };

    #[test]
    fn check_alloc_ownership() {
        let mut buf = [0u8; 43]; // Room for 10 u32s, and extra space.
        let mut alloc = StackAlloc::new(&mut buf);
        // Someday, NLL will eliminate the need for scopes here.
        {
            let _ = Vec::<u32>::new(&mut alloc);
        }
        // Make sure we can reuse the alloc.
        {
            let _ = Vec::<u32>::new(&mut alloc);
        }
    }

    #[test]
    fn check_one_push_works() {
        let mut buf = [0u8; 43]; // Room for 10 u32s, and extra space.
        let mut alloc = StackAlloc::new(&mut buf);
        let mut v = Vec::<u32>::new(&mut alloc);
        v.push(1).expect("v.push(1) failed.");

        assert_eq!(v.deref(), &[1]);
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
        assert_eq!(v.deref(), &[1, 2, 3, 4]);
    }

    #[test]
    fn check_two_vectors_one_alloc() {
        let mut buf = [0u8; 56];
        let mut alloc = StackAlloc::new(&mut buf);
        let mut v = Vec::<u32>::new(&mut alloc);
        let mut w = Vec::<u32>::new(&mut alloc);

        v.push(1).expect("v.push(1) failed.");
        w.push(11).expect("w.push(11) failed.");

        v.push(2).expect("v.push(2) failed.");
        w.push(22).expect("w.push(22) failed.");

        v.push(3).expect("v.push(3) failed.");
        w.push(33).expect("w.push(33) failed.");

        v.push(4).expect("v.push(4) failed.");
        w.push(44).expect("w.push(44) failed.");

        assert_eq!(v.deref(), &[1, 2, 3, 4]);
        assert_eq!(w.deref(), &[11, 22, 33, 44]);
        assert_eq!(alloc.bytes_in_use(), alloc.capacity());
    }

}
