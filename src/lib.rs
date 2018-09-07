#![feature(allocator_api)]

#![deny(warnings)]

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

pub struct Vec<'alloc, T> {
    ptr:  NonNull<T>,
    cap:   usize,
    len:   usize,
    alloc: &'alloc mut dyn alloc::Alloc,
}

impl <'alloc, T> Vec<'alloc, T>
{

    pub fn new(alloc: &'alloc mut impl alloc::Alloc) -> Self {
        Vec {
            ptr: NonNull::dangling(),
            cap: 0,
            len: 0,
            alloc,
        }
    }

    pub fn push(&mut self, elem: T) {
        if self.len == self.cap {
            self.grow();
        }
        unsafe {
            ptr::write(self.ptr.as_ptr().offset(self.len as isize), elem);
        }
        self.len += 1;
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

    fn grow(&mut self) {
        // There's lots of room for error here, so let's call it all unsafe.
        // unsafe
        {
            let new_cap: usize;
            let new_ptr: NonNull<T>;

            if self.cap == 0 {
                new_cap = 1;
                // new_ptr = self.alloc.alloc_array(1).unwrap();
            } else {
                new_cap = 2 * self.cap;
                // new_ptr = self.alloc.
                //             realloc_array(self.ptr, self.cap, new_cap).unwrap();
            }
            new_ptr = self.ptr;

            self.cap = new_cap;
            self.ptr = new_ptr;
        }
    }

}

impl <'alloc, T> Drop for Vec<'alloc, T> {

    fn drop(&mut self) {
        if self.cap != 0 {
            // We must call each destructor. If T doesn't impl Drop, this loop
            // is optimized out.
            while let Some(_) = self.pop() {};

            unsafe {
                let layout = alloc::Layout::array::<T>(self.cap).unwrap();
                self.alloc.dealloc(self.ptr.cast(), layout);
            }
        }
    }

}

impl <'alloc, T> ops::Deref for Vec<'alloc, T> {

    type Target = [T];

    fn deref(&self) -> &[T] {
        unsafe {
            slice::from_raw_parts(self.ptr.as_ptr(), self.len)
        }
    }

}

impl <'alloc, T> ops::DerefMut for Vec<'alloc, T> {

    fn deref_mut(&mut self) -> &mut [T] {
        unsafe {
            slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len)
        }
    }

}


pub struct SliceAlloc<'a> {
    _buf:  &'a [u8],
    _top:  usize,
    _high: usize,
}

impl <'a> SliceAlloc<'a> {

    pub fn new(_buf: &[u8]) -> SliceAlloc {
        SliceAlloc {
            _buf,
            _top:  0,
            _high: 0,
        }
    }

}

unsafe impl <'a> alloc::Alloc for SliceAlloc<'a> {

    unsafe fn alloc(&mut self, _layout: alloc::Layout)
        -> result::Result<NonNull<u8>, alloc::AllocErr>
    {
        // Adjust top for alignment
        // Save that top as the returned pointer
        // Adjust top for the size
        // Check bounds
        // If Ok(), return saved top
        Ok(NonNull::<u8>::dangling()) // lol bad idea
    }

    unsafe fn dealloc(&mut self, _ptr: NonNull<u8>, _layout: alloc::Layout) {
        // TODO: Check if this allocation is the last one made.
        //       If this layout and ptr are at the top of the stack, we can
        //       dealloc it.
        //       Otherwise we can't do anything, ever.
        //       We could keep track of which sections are allocated or not...
        () // Do nothing.
    }

}

#[cfg(test)]
mod t {

    use super::*;

    #[allow(unused_imports)]
    use std::{
        ops::Deref,
    };

    #[test]
    fn check_new_and_drop() {
        let buf = [0u8; 43];
        let mut alloc = SliceAlloc::new(&buf);
        // Use scope to trigger drops, because NLL are a pipe dream.
        {
            let _ = Vec::<u32>::new(&mut alloc);
        }
        // Make sure we can reuse the alloc.
        {
            let _ = Vec::<u32>::new(&mut alloc);
        }
    }

    #[test]
    fn check_push_alloc() {
        let buf = [0u8; 43];
        let mut alloc = SliceAlloc::new(&buf);
        let mut v = Vec::<u32>::new(&mut alloc);

        // Only trigger one alloc.
        v.push(1);

        assert_eq!(v.deref(), &[1]);
    }

    #[test]
    fn check_push_realloc() {
        let buf = [0u8; 43]; // Room for 10 u32s, and extra space.
        let mut alloc = SliceAlloc::new(&buf);
        let mut v = Vec::<u32>::new(&mut alloc);

        // Trigger at least realloc.
        v.push(1);
        v.push(2);
        v.push(3);
        v.push(4);
        v.push(5);

        assert_eq!(v.deref(), &[1, 2, 3, 4, 5]);
    }

}
