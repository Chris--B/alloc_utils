#![feature(allocator_api)]

#![deny(warnings)]

#[cfg(test)]
#[macro_use]
extern crate pretty_assertions;

pub mod stack_alloc;
pub mod vec2;
