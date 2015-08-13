TARGET(elf32-i386)

INPUT(beta.o) /* assembly stage1 */
INPUT(gamma.o)  /* rust code */
INPUT(libstd.o)
INPUT(libcore.o)
INPUT(libbootalloc.o)
INPUT(liballoc.o)
INPUT(libcollections.o)
INPUT(librustc_unicode.o)
INPUT(librlibc.o)
INPUT(libdisk.o)

OUTPUT_ARCH(i386)
OUTPUT_FORMAT("binary")

SECTIONS
{
    /DISCARD/ : {
        * (.eh_frame)
    }

    text 0x7c00 :
    {
        beta.o (*)
        gamma.o (.text.rust_boot)
        * (*)
    }
}

