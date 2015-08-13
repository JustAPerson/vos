// #![feature(slice_bytes, no_std)]
// #![no_std]

#![feature(asm)]
#![feature(slice_bytes)]

#[cfg(not(feature="bootdriver"))]
#[macro_use]
extern crate log;

#[cfg(feature="bootdriver")]
macro_rules! debug {
    ($fmt:expr) => { };
    ($fmt:expr, $( $arg:expr ),* ) => { };
}


mod byteorder;
mod disk;
pub mod fs;

pub use disk::*;
pub use fs::*;
