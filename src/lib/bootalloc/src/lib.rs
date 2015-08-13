#![feature(no_std)]
#![no_std]

static mut PTR: usize = 0xa00;

#[inline]
pub fn allocate(size: usize, _: usize) -> *mut u8 {
    unsafe {
        let ptr = PTR;
        PTR += size;
        ptr as *mut u8
    }
}

#[inline]
pub fn reallocate(_: *mut u8, _: usize, size: usize, align: usize) -> *mut u8 {
    allocate(size, align)
}

#[inline]
pub fn reallocate_inplace(_: *mut u8, _: usize, size: usize,
                                 align: usize) -> usize {
    allocate(size, align) as usize
}

pub unsafe fn deallocate(_: *mut u8, _: usize, _: usize) { }
pub fn usable_size(size: usize, _: usize) -> usize {
    size
}
pub fn stats_print() { }

