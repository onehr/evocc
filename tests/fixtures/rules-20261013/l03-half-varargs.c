/* L03 probe: one anonymous _Float16 (KIND=0) or __bf16 (KIND=2) argument.
 * Build the callee with -DROLE=0 and the caller with -DROLE=1, possibly at
 * different optimization levels or with different compilers, and link them.
 * The caller passes 1.5 and prints what the callee read through a local and
 * through a call: "3e00 3e00" (_Float16) or "3fc0 3fc0" (__bf16) means both
 * sides agree. This probe prints, so it serves the reference compilers. */
#include <stdarg.h>
#include <stdio.h>
#include <string.h>
#ifndef KIND
#define KIND 0
#endif
#if KIND == 0
typedef _Float16 H;
#elif KIND == 2
typedef __bf16 H;
#else
#error unsupported KIND
#endif

unsigned short via_local(int n, ...);
unsigned short via_call(int n, ...);

#if ROLE == 0
__attribute__((noinline)) static unsigned short bitsof(H h)
{
    unsigned short u;
    memcpy(&u, &h, sizeof u);
    return u;
}

unsigned short via_local(int n, ...)
{
    va_list ap;
    va_start(ap, n);
    H h = va_arg(ap, H);
    va_end(ap);
    unsigned short u;
    memcpy(&u, &h, sizeof u);
    return u;
}

unsigned short via_call(int n, ...)
{
    va_list ap;
    va_start(ap, n);
    unsigned short u = bitsof(va_arg(ap, H));
    va_end(ap);
    return u;
}
#else
int main(void)
{
    unsigned short in = KIND == 0 ? 0x3e00 : 0x3fc0;
    H h;
    memcpy(&h, &in, sizeof h);
    printf("%04x %04x\n", via_local(1, h), via_call(1, h));
    return 0;
}
#endif
