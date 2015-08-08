#[inline(always)]
pub unsafe fn clear_screen() {
    asm!("mov ax, 0x0003
          int 0x10" :::"{ax}":"intel");
}

#[inline(always)]
pub unsafe fn print_string(s: &str) {
    extern "rust-intrinsic" {
        fn transmute<T, U>(_: T) -> U;
    }

    let (ptr, len): (*const u8, usize) = transmute::<_, _>(s);
    asm!("mov ah, 0x0E
          mov bx, 0x0007
          .loop:
          mov al, [si]
          inc si
          int 0x10
          loop .loop"
          : // no outputs
          : "{si}"(ptr), "{cx}"(len)// input
          : "ax", "bx" // clober
          : "intel");
}

