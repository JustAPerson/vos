#![crate_type="lib"]
#![feature(no_std, lang_items, asm, intrinsics)]
#![no_std]

#[no_mangle]
pub fn boot() {
    unsafe {
        bios::clear_screen();
        bios::print_string(MESSAGE);
    }

    // prevent CPU executing past this point
    // anything beyond this is likely an invalid instruction sequence
    loop {}

    static MESSAGE: &'static str  = "Hello World from Rust!";
}

mod bios;

// all of this is required due to no libstd
pub mod lang {
    #[no_mangle]
    pub fn __morestack() {}

    #[lang="panic_fmt"]
    fn panic_fmt() {}
    #[lang="stack_exhausted"]
    fn stack_exhausted() {}
    #[lang="eh_personality"]
    fn eh_personality() {}

    // below no longer necessary once we get libcore
    #[lang="sized"]
    trait Sized {}
    #[lang="copy"]
    trait Copy {}
    #[lang="sync"]
    trait Sync {}

    impl Sync for &'static str {}
}
