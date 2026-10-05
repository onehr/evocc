// evocc-native-ub-checks=checked
.text

.globl FixedMul
.p2align 2
.type FixedMul, %function
FixedMul:
    stp x29, x30, [sp, #-16]!
    mov x29, sp
.LFixedMul_block0:
    sxtw x0, w0
    sxtw x1, w1
    mul x2, x0, x1
    smulh x3, x0, x1
    asr x2, x2, #63
    cmp x3, x2
    b.ne .LFixedMul_block1
.LFixedMul_block2:
    mul x0, x0, x1
    asr x0, x0, #16
    mov w0, w0
    ldp x29, x30, [sp], #16
    ret
.LFixedMul_block1:
    brk #3004
.size FixedMul, .-FixedMul

.ident "EvoCC 0.0.0 aarch64-unknown-linux-gnu"
