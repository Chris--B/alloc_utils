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
        iter::Iterator,
        mem::size_of,
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
        let mut buf = [0u8; 2 * size_of::<u32>()];
        let buf_len = buf.len(); // Good job borrow checker!
        let mut alloc = StackAlloc::new(&mut buf);

        let layout = alloc::Layout::new::<u32>();
        // This *should* be knowable at compile time, but Rust isn't there yet.
        assert_eq!(2 * layout.size(), buf_len);

        // We expect two allocations to work, and then two to fail.
        // Failure should *not* abort the test!
        let ptr_pairs;
        unsafe {
            ptr_pairs = [
                (alloc.alloc(layout), R::Ok),
                (alloc.alloc(layout), R::Ok),

                (alloc.alloc(layout), R::Err),
                (alloc.alloc(layout), R::Err),
            ];

            // Some day...
            //      let tags: impl Iterator<Item=R>
            //      And assert_eq can just run through the iterator.
            // Until then:
            // Note that we use std::Vec here, with the global allocator.
            let expected_tags: Vec<R> = ptr_pairs
                .iter()
                .map(|pair| pair.1)
                .collect();
            let actual_tags: Vec<R>   = ptr_pairs
                .iter()
                .map(|pair| R::from_result(&pair.0))
                .collect();
            assert_eq!(expected_tags, actual_tags);
        }

    }

}
