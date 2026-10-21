// A bare-metal harness for qemu-system-aarch64 -M virt (D31). It runs at EL1
// with the MMU off, so every data access is a Device access, and it also sets
// SCTLR_EL1.A. Each access that is not aligned to its size then takes an
// alignment fault. The harness calls main and leaves QEMU through semihosting
// with main's low 8 bits as the exit status. A fault prints ESR_EL1, FAR_EL1
// and ELR_EL1 and leaves with status 200.
    .section .text.start, "ax"
    .globl _start
_start:
    ldr x0, =__stack_top
    mov sp, x0
    // CPACR_EL1.FPEN = 0b11: floating-point and SIMD instructions run.
    mov x0, #(3 << 20)
    msr cpacr_el1, x0
    isb
    ldr x0, =vectors
    msr vbar_el1, x0
    // SCTLR_EL1.A = 1; the MMU and the caches stay off.
    mrs x0, sctlr_el1
    orr x0, x0, #2
    msr sctlr_el1, x0
    isb
    bl main
    and x0, x0, #0xff
    b exit_with

// Semihosting SYS_EXIT with ADP_Stopped_ApplicationExit and the status in x0.
exit_with:
    adr x1, exit_block
    mov x2, #0x26
    movk x2, #2, lsl #16
    str x2, [x1]
    str x0, [x1, #8]
    mov w0, #0x18
    hlt #0xf000
1:  b 1b

    .balign 16
exit_block:
    .quad 0, 0

    .balign 2048
vectors:
    .rept 16
    .balign 128
    b fault
    .endr

fault:
    mrs x19, esr_el1
    mrs x20, far_el1
    mrs x21, elr_el1
    adr x0, banner
    bl write0
    mov x0, x19
    bl write_hex
    mov x0, x20
    bl write_hex
    mov x0, x21
    bl write_hex
    mov x0, #200
    b exit_with

// Semihosting SYS_WRITE0 of the string at x0.
write0:
    mov x1, x0
    mov w0, #0x04
    hlt #0xf000
    ret

// x0 as sixteen hexadecimal digits and a newline.
write_hex:
    adr x1, digits
    mov x2, #60
2:  lsr x3, x0, x2
    and x3, x3, #15
    cmp x3, #10
    add x4, x3, #'0'
    add x5, x3, #('a' - 10)
    csel x3, x4, x5, lo
    strb w3, [x1], #1
    subs x2, x2, #4
    b.pl 2b
    mov w3, #10
    strb w3, [x1], #1
    strb wzr, [x1]
    adr x1, digits
    mov w0, #0x04
    hlt #0xf000
    ret

banner:
    .asciz "FAULT esr far elr\n"
    .balign 8
digits:
    .space 32
