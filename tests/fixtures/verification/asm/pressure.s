// evocc-native-ub-checks=checked
.text

.globl wide
.p2align 2
.type wide, %function
wide:
    stp x29, x30, [sp, #-16]!
    mov x29, sp
    sub sp, sp, #144
    str x19, [sp, #56]
    str x20, [sp, #64]
    str x21, [sp, #72]
    str x22, [sp, #80]
    str x23, [sp, #88]
    str x24, [sp, #96]
    str x25, [sp, #104]
    str x26, [sp, #112]
    str x27, [sp, #120]
    str x28, [sp, #128]
.Lwide_block0:
    ldr x8, [x29, #16]
    ldr x9, [x29, #24]
    ldr x10, [x29, #32]
    ldr x11, [x29, #40]
    ldr x12, [x29, #48]
    ldr x13, [x29, #56]
    ldr x14, [x29, #64]
    ldr x15, [x29, #72]
    ldr x19, [x29, #80]
    ldr x20, [x29, #88]
    ldr x21, [x29, #96]
    ldr x22, [x29, #104]
    ldr x23, [x29, #112]
    ldr x24, [x29, #120]
    ldr x25, [x29, #128]
    ldr x26, [x29, #136]
    ldr x27, [x29, #144]
    ldr x28, [x29, #152]
    str x28, [sp, #48]
    ldr x28, [x29, #160]
    str x28, [sp, #0]
    ldr x28, [x29, #168]
    str x28, [sp, #8]
    ldr x28, [x29, #176]
    str x28, [sp, #16]
    ldr x28, [x29, #184]
    str x28, [sp, #24]
    ldr x28, [x29, #192]
    str x28, [sp, #32]
    ldr x28, [x29, #200]
    str x28, [sp, #40]
    add x0, x0, x1
    add x0, x0, x2
    add x0, x0, x3
    add x0, x0, x4
    add x0, x0, x5
    add x0, x0, x6
    add x0, x0, x7
    add x0, x0, x8
    add x0, x0, x9
    add x0, x0, x10
    add x0, x0, x11
    add x0, x0, x12
    add x0, x0, x13
    add x0, x0, x14
    add x0, x0, x15
    add x0, x0, x19
    add x0, x0, x20
    add x0, x0, x21
    add x0, x0, x22
    add x0, x0, x23
    add x0, x0, x24
    add x0, x0, x25
    add x0, x0, x26
    add x0, x0, x27
    ldr x1, [sp, #48]
    add x0, x0, x1
    ldr x1, [sp, #0]
    add x0, x0, x1
    ldr x1, [sp, #8]
    add x0, x0, x1
    ldr x1, [sp, #16]
    add x0, x0, x1
    ldr x1, [sp, #24]
    add x0, x0, x1
    ldr x1, [sp, #32]
    add x0, x0, x1
    ldr x1, [sp, #40]
    add x0, x0, x1
    ldr x19, [sp, #56]
    ldr x20, [sp, #64]
    ldr x21, [sp, #72]
    ldr x22, [sp, #80]
    ldr x23, [sp, #88]
    ldr x24, [sp, #96]
    ldr x25, [sp, #104]
    ldr x26, [sp, #112]
    ldr x27, [sp, #120]
    ldr x28, [sp, #128]
    add sp, sp, #144
    ldp x29, x30, [sp], #16
    ret
.size wide, .-wide

.globl pressure
.p2align 2
.type pressure, %function
pressure:
    stp x29, x30, [sp, #-16]!
    mov x29, sp
    sub sp, sp, #336
    str x19, [sp, #256]
    str x20, [sp, #264]
    str x21, [sp, #272]
    str x22, [sp, #280]
    str x23, [sp, #288]
    str x24, [sp, #296]
    str x25, [sp, #304]
    str x26, [sp, #312]
    str x27, [sp, #320]
    str x28, [sp, #328]
.Lpressure_block0:
    add x1, x0, #0
    add x2, x0, #1
    add x3, x0, #2
    add x4, x0, #3
    add x5, x0, #4
    add x6, x0, #5
    add x7, x0, #6
    add x8, x0, #7
    add x9, x0, #8
    add x10, x0, #9
    add x11, x0, #10
    add x12, x0, #11
    add x13, x0, #12
    add x14, x0, #13
    add x15, x0, #14
    add x19, x0, #15
    add x20, x0, #16
    add x21, x0, #17
    add x22, x0, #18
    add x23, x0, #19
    add x24, x0, #20
    add x25, x0, #21
    add x26, x0, #22
    add x27, x0, #23
    add x28, x0, #24
    str x28, [sp, #248]
    add x28, x0, #25
    str x28, [sp, #192]
    add x28, x0, #26
    str x28, [sp, #200]
    add x28, x0, #27
    str x28, [sp, #208]
    add x28, x0, #28
    str x28, [sp, #216]
    add x28, x0, #29
    str x28, [sp, #224]
    add x28, x0, #30
    str x28, [sp, #232]
    add x0, x0, #31
    str x0, [sp, #240]
    mov x0, x1
    mov x1, x2
    mov x2, x3
    mov x3, x4
    mov x4, x5
    mov x5, x6
    mov x6, x7
    mov x7, x8
    str x9, [sp, #0]
    str x10, [sp, #8]
    str x11, [sp, #16]
    str x12, [sp, #24]
    str x13, [sp, #32]
    str x14, [sp, #40]
    str x15, [sp, #48]
    str x19, [sp, #56]
    str x20, [sp, #64]
    str x21, [sp, #72]
    str x22, [sp, #80]
    str x23, [sp, #88]
    str x24, [sp, #96]
    str x25, [sp, #104]
    str x26, [sp, #112]
    str x27, [sp, #120]
    ldr x8, [sp, #248]
    str x8, [sp, #128]
    ldr x8, [sp, #192]
    str x8, [sp, #136]
    ldr x8, [sp, #200]
    str x8, [sp, #144]
    ldr x8, [sp, #208]
    str x8, [sp, #152]
    ldr x8, [sp, #216]
    str x8, [sp, #160]
    ldr x8, [sp, #224]
    str x8, [sp, #168]
    ldr x8, [sp, #232]
    str x8, [sp, #176]
    ldr x8, [sp, #240]
    str x8, [sp, #184]
    bl wide
    ldr x19, [sp, #256]
    ldr x20, [sp, #264]
    ldr x21, [sp, #272]
    ldr x22, [sp, #280]
    ldr x23, [sp, #288]
    ldr x24, [sp, #296]
    ldr x25, [sp, #304]
    ldr x26, [sp, #312]
    ldr x27, [sp, #320]
    ldr x28, [sp, #328]
    add sp, sp, #336
    ldp x29, x30, [sp], #16
    ret
.size pressure, .-pressure

.ident "EvoCC 0.0.0 aarch64-unknown-linux-gnu"
