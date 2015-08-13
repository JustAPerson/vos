#![crate_type="lib"]
#![feature(asm, intrinsics)]

#[no_mangle]
pub fn rust_boot() {
    unsafe {
        bios::print_string("Hello from Rust! Yay!!");
    }

    let mut disk = disk::BiosDisk::new();
    let mut fs = disk::mount(&mut disk, 0).unwrap(); // TODO mount correct partition
    // TODO unwrap()????

    fs.read_file("/kernel.img".as_ref());

    // prevent CPU executing past this point
    // anything beyond this is likely an invalid instruction sequence
    loop {}
}

mod bios;
extern crate disk;
