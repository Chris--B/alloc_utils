use std::{
    alloc,
    result,
    ptr::NonNull,
};

pub struct StackAlloc<'a> {
    _buf:  &'a [u8],
    top:  usize,
    _high: usize,
}

pub struct Marker(usize);

impl <'a> StackAlloc<'a> {

    pub fn new(_buf: &mut [u8]) -> StackAlloc {
        StackAlloc {
            _buf,
            top:  0,
            _high: 0,
        }
    }

    pub fn get_marker(&self) -> Marker {
        Marker(self.top)
    }

}

unsafe impl <'a> alloc::Alloc for StackAlloc<'a> {

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
        let buf_len = buf.len(); // Good job borrow checker!

        // The pointers we expect to be valid are saved here, and used at the
        // end of the function.
        let ptrs: [NonNull<u8>; 2];

        // NLL cannot come quickly enough.
        // Force alloc to drop before we check our pointers at the end, because
        // alloc &muts buf, and we need to read buf to check the tests.
        // Some day, this can just use a mem::forget() call instead of scoping.
        {
            let mut alloc = StackAlloc::new(&mut buf);

            let layout = alloc::Layout::new::<u32>();
            // This *should* be knowable at compile time, but Rust isn't there yet.
            assert_eq!(2 * layout.size(), buf_len);

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

                assert_eq!(expected_tags, actual_tags);

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
            *a = 0x00;
            assert_eq!(buf[0], *a as u8);
            *a = 0xff;
            assert_eq!(buf[0], *a as u8);

            let b: &mut u32 = (ptrs[1].as_ptr() as *mut u32).as_mut().unwrap();
            *b = 0x00;
            assert_eq!(buf[1], *b as u8);
            *b = 0xff;
            assert_eq!(buf[1], *b as u8);
        }
    }

}
