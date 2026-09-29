.section __TEXT,__text,regular,pure_instructions

.globl _simplify
.p2align 2
_simplify:
    stp x29, x30, [sp, #-16]!
    mov x29, sp
Lsimplify_block0:
    ldp x29, x30, [sp], #16
    ret

.globl _main
.p2align 2
_main:
    stp x29, x30, [sp, #-16]!
    mov x29, sp
Lmain_block0:
    movz w0, #17
    bl _simplify
    cmp w0, #17
    cset w0, ne
    and w0, w0, #1
    ldp x29, x30, [sp], #16
    ret
