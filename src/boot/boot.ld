TARGET(elf32-i386)
INPUT(alpha.o)

OUTPUT_ARCH(i386)
OUTPUT_FORMAT("binary")

ENTRY(boot)

bios_load_point = 0x7c00;

SECTIONS
{
    . = bios_load_point;
    stage1.text :
    AT(bios_load_point)
    {
        alpha.o (.text)
    }

    . = bios_load_point + 0x1FE;
    stage1.magic : {
        SHORT(0xAA55)
    }
}

