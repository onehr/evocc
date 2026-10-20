// checkpoint: a returns-twice function. A tail branch keeps the caller's
// return address and stack pointer, so _setjmp saves the caller's context.
    .text
    .globl _checkpoint
    .p2align 2
_checkpoint:
    b __setjmp
