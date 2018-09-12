#![feature(allocator_api)]

#![deny(warnings)]

#[cfg(test)]
#[macro_use]
extern crate pretty_assertions;

pub mod raw_vec;
pub mod stack_alloc;
pub mod vec2;
