// checkpoint: a returns-twice function. A tail branch keeps the caller's
// return address and stack pointer, so _setjmp saves the caller's context.
    .text
    .globl checkpoint
    .type checkpoint, %function
    .p2align 2
checkpoint:
    b _setjmp
    .size checkpoint, .-checkpoint
    .section .note.GNU-stack,"",%progbits
