#![feature(alloc)]
#![feature(collections)]
#![feature(core)]
#![feature(core_intrinsics)]
#![feature(core_panic)]
#![feature(core_simd)]
#![feature(into_cow)]
#![feature(iter_order)]
#![feature(macro_reexport)]
#![feature(no_std)]
#![feature(raw)]
#![feature(slice_concat_ext)]
#![feature(staged_api)]
#![feature(unicode)]
#![feature(vec_push_all)]


#![no_std]

#[macro_reexport(assert, assert_eq, debug_assert, debug_assert_eq, panic,
    unreachable, unimplemented, write, writeln)]
extern crate core as __core;

// extern crate bootalloc;
extern crate rustc_unicode;
extern crate alloc;
extern crate collections;


pub use core::any;
pub use core::cell;
pub use core::clone;
pub use core::cmp;
pub use core::convert;
pub use core::default;
pub use core::hash;
pub use core::intrinsics;
pub use core::iter;
pub use core::marker;
pub use core::mem;
pub use core::ops;
pub use core::ptr;
pub use core::raw;
pub use core::simd;
pub use core::result;
pub use core::option;

pub use alloc::boxed;
pub use alloc::rc;

pub use collections::borrow;
pub use collections::fmt;
pub use collections::slice;
pub use collections::str;
pub use collections::string;
pub use collections::vec;

pub use rustc_unicode::char;

pub mod ffi;
pub mod path;
pub mod sys;
pub mod sys_common;

pub mod panicking {
    pub use core::panicking::panic;
    pub use core::panicking::panic_fmt;
}
pub mod prelude {
    pub mod v1 {
        pub use marker::{Copy, Send, Sized, Sync};
        pub use ops::{Drop, Fn, FnMut, FnOnce};
        pub use mem::drop;
        pub use boxed::Box;
        pub use borrow::ToOwned;
        pub use clone::Clone;
        pub use cmp::{PartialEq, PartialOrd, Eq, Ord};
        pub use convert::{AsRef, AsMut, Into, From};
        pub use default::Default;
        pub use iter::{Iterator, Extend, IntoIterator};
        pub use iter::{DoubleEndedIterator, ExactSizeIterator};
        pub use option::Option::{self, Some, None};
        pub use result::Result::{self, Ok, Err};
        pub use slice::SliceConcatExt;
        pub use string::{String, ToString};
        pub use vec::Vec;
    }
}

#[macro_export]
macro_rules! try {
    ($expr:expr) => (match $expr {
        $crate::result::Result::Ok(val) => val,
        $crate::result::Result::Err(err) => {
            return $crate::result::Result::Err($crate::convert::From::from(err))
        }
    })
}

// TODO: panic with hardware interrupt or write to port
// #[macro_export]
// macro_rules! panic {
//     ($fmt:expr) => { };
//     ($fmt:expr, $( $arg:expr ),* ) => { };
// }
