/* The image of start.s and the program, in the RAM of qemu-system-aarch64
   -M virt, with a 1 MiB stack above it. */
ENTRY(_start)
SECTIONS {
    . = 0x40080000;
    .text : { *(.text.start) *(.text*) }
    .rodata : { *(.rodata*) }
    .data : { *(.data*) }
    .bss : { *(.bss*) *(COMMON) }
    . = ALIGN(16);
    . = . + 0x100000;
    __stack_top = .;
}
