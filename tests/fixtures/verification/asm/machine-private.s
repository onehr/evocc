// evocc-native-ub-checks=checked
.text

.globl private_word
.p2align 2
.type private_word, %function
private_word:
    stp x29, x30, [sp, #-16]!
    mov x29, sp
    sub sp, sp, #16
.Lprivate_word_block0:
    add x1, sp, #0
    str w0, [x1, #0]
    ldr w0, [x1, #0]
    add sp, sp, #16
    ldp x29, x30, [sp], #16
    ret
.size private_word, .-private_word

.globl private_wide
.p2align 2
.type private_wide, %function
private_wide:
    stp x29, x30, [sp, #-16]!
    mov x29, sp
    sub sp, sp, #16
.Lprivate_wide_block0:
    add x1, sp, #0
    str x0, [x1, #0]
    ldr x0, [x1, #0]
    add sp, sp, #16
    ldp x29, x30, [sp], #16
    ret
.size private_wide, .-private_wide

.globl private_caller
.p2align 2
.type private_caller, %function
private_caller:
    stp x29, x30, [sp, #-16]!
    mov x29, sp
    sub sp, sp, #16
    str x19, [sp, #8]
.Lprivate_caller_block0:
    add x19, sp, #0
    str w0, [x19, #0]
    bl private_word
    ldr w1, [x19, #0]
    add w0, w0, w1
    ldr x19, [sp, #8]
    add sp, sp, #16
    ldp x29, x30, [sp], #16
    ret
.size private_caller, .-private_caller

.globl private_loop
.p2align 2
.type private_loop, %function
private_loop:
    stp x29, x30, [sp, #-16]!
    mov x29, sp
    sub sp, sp, #16
.Lprivate_loop_block0:
    add x2, sp, #0
    str w1, [x2, #0]
.Lprivate_loop_block1:
    cmp w0, #0
    b.eq .Lprivate_loop_block3
.Lprivate_loop_block2:
    ldr w1, [x2, #0]
    add w1, w1, #1
    sub w3, w0, #1
    str w1, [x2, #0]
    mov w0, w3
    b .Lprivate_loop_block1
.Lprivate_loop_block3:
    ldr w0, [x2, #0]
    add sp, sp, #16
    ldp x29, x30, [sp], #16
    ret
.size private_loop, .-private_loop

.ident "EvoCC 0.0.0 aarch64-unknown-linux-gnu"
