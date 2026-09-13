SECTIONS
{
    .rodata :
    {
        __readonly_start = .;
        *(.rodata .rodata.*)
        __readonly_end = .;
    }
}
INSERT AFTER .text;
