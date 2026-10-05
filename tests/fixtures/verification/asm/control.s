// evocc-native-ub-checks=checked
.text

.globl loop_join
.p2align 2
.type loop_join, %function
loop_join:
    stp x29, x30, [sp, #-16]!
    mov x29, sp
.Lloop_join_block0:
    movz w2, #0
    movz w3, #0
    mov w4, w0
.Lloop_join_block1:
    cmp w3, w1
    b.hs .Lloop_join_block20
.Lloop_join_block2:
    and w0, w3, #3
    cmp w0, #0
    b.eq .Lloop_join_block21
.Lloop_join_block3:
    cmp w3, #17
    b.eq .Lloop_join_block22
.Lloop_join_block4:
    and w0, w4, #1
    cmp w0, #0
    b.eq .Lloop_join_block6
.Lloop_join_block5:
    mov w0, w3
    b .Lloop_join_block8
.Lloop_join_block6:
    eor w5, w3, w4
    mov w0, w5
    b .Lloop_join_block8
.Lloop_join_block7:
    add w7, w3, #1
    mov w3, w7
    mov w4, w5
    mov w2, w6
    b .Lloop_join_block1
.Lloop_join_block8:
    add w0, w2, w0
    movz w7, #1
    cmp w7, #32
    b.hs .Lloop_join_block23
.Lloop_join_block17:
    lsr w7, w4, #1
    movz w8, #31
    cmp w8, #32
    b.hs .Lloop_join_block24
.Lloop_join_block18:
    lsl w8, w4, #31
    orr w7, w7, w8
    mov w5, w7
    mov w6, w0
    b .Lloop_join_block7
.Lloop_join_block9:
    mov w0, w1
.Lloop_join_block10:
    cmp w0, #20
    b.ls .Lloop_join_block12
.Lloop_join_block11:
    sub w7, w0, #7
    mov w0, w7
    b .Lloop_join_block10
.Lloop_join_block12:
    mov w7, w0
    mov w8, w2
.Lloop_join_block13:
    add w9, w8, w7
    movz w0, #1
    cmp w0, #32
    b.hs .Lloop_join_block25
.Lloop_join_block19:
    lsr w10, w7, #1
.Lloop_join_block14:
    cmp w10, #0
    b.ne .Lloop_join_block26
.Lloop_join_block15:
    mov w0, w9
    ldp x29, x30, [sp], #16
    ret
.Lloop_join_block16:
    brk #3005
.Lloop_join_block20:
    b .Lloop_join_block9
.Lloop_join_block21:
    mov w5, w4
    mov w6, w2
    b .Lloop_join_block7
.Lloop_join_block22:
    b .Lloop_join_block9
.Lloop_join_block23:
    b .Lloop_join_block16
.Lloop_join_block24:
    b .Lloop_join_block16
.Lloop_join_block25:
    b .Lloop_join_block16
.Lloop_join_block26:
    mov w7, w10
    mov w8, w9
    b .Lloop_join_block13
.size loop_join, .-loop_join

.globl short_circuit
.p2align 2
.type short_circuit, %function
short_circuit:
    stp x29, x30, [sp, #-16]!
    mov x29, sp
.Lshort_circuit_block0:
    movz w1, #0
    cmp w0, #0
    b.eq .Lshort_circuit_block2
.Lshort_circuit_block1:
    movz w2, #3
    cmp w2, #0
    cset w3, ne
    b .Lshort_circuit_block3
.Lshort_circuit_block2:
    movz w4, #0
    mov w2, w1
    mov w3, w4
.Lshort_circuit_block3:
    tbz w3, #0, .Lshort_circuit_block5
.Lshort_circuit_block4:
    movz w1, #1
    adds w1, w2, w1
    b.vs .Lshort_circuit_block16
.Lshort_circuit_block14:
    b .Lshort_circuit_block6
.Lshort_circuit_block5:
    mov w1, w2
.Lshort_circuit_block6:
    cmp w0, #0
    b.eq .Lshort_circuit_block8
.Lshort_circuit_block7:
    movz w0, #9
    cmp w0, #0
    cset w2, ne
    b .Lshort_circuit_block9
.Lshort_circuit_block8:
    movz w3, #1
    mov w0, w1
    mov w2, w3
.Lshort_circuit_block9:
    tbz w2, #0, .Lshort_circuit_block11
.Lshort_circuit_block10:
    movz w1, #2
    adds w1, w0, w1
    b.vs .Lshort_circuit_block17
.Lshort_circuit_block15:
    b .Lshort_circuit_block12
.Lshort_circuit_block11:
    mov w1, w0
.Lshort_circuit_block12:
    mov w0, w1
    ldp x29, x30, [sp], #16
    ret
.Lshort_circuit_block13:
    brk #3004
.Lshort_circuit_block16:
    b .Lshort_circuit_block13
.Lshort_circuit_block17:
    b .Lshort_circuit_block13
.size short_circuit, .-short_circuit

.ident "EvoCC 0.0.0 aarch64-unknown-linux-gnu"
