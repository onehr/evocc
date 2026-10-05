// evocc-native-ub-checks=checked
.text

.globl abs
.p2align 2
.type abs, %function
abs:
    stp x29, x30, [sp, #-16]!
    mov x29, sp
.Labs_block0:
    cmp w0, #0
    b.le .Labs_block2
.Labs_block1:
    mov w1, w0
    b .Labs_block3
.Labs_block2:
    movz w2, #0
    subs w0, w2, w0
    b.vs .Labs_block4
.Labs_block5:
    mov w1, w0
.Labs_block3:
    mov w0, w1
    ldp x29, x30, [sp], #16
    ret
.Labs_block4:
    brk #3004
.size abs, .-abs

.ident "EvoCC 0.0.0 aarch64-unknown-linux-gnu"
