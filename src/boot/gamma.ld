TARGET(elf32-i386)

INPUT(beta.o) /* assembly stage1 */
INPUT(gamma.o)  /* rust code */

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
        gamma.o (*)
    }
}

