/* L11c: the padding bytes of a naturally aligned record take part in a
 * generic compare-exchange, and load, store and exchange move them
 * unchanged. CASE=0 and CASE=2 return 0. CASE=1 leaves a padding byte of
 * the expected record uninitialized; both interpreters report E3001 at the
 * comparison. l11c-aggregate.c CASE=4 has the same record with alignment 2,
 * which the rule refuses as misaligned (E2913). */
#include <string.h>
#ifndef CASE
#define CASE 0
#endif
struct __attribute__((aligned(4))) padded { char c; short s; };

int main(void)
{
#if CASE == 0 || CASE == 1
    struct padded cell, want, next;
    memset(&cell, 0, sizeof cell);
#if CASE == 0
    memset(&want, 0xff, sizeof want);
#endif
    memset(&next, 0, sizeof next);
    cell.c = 1; cell.s = 2; want.c = 1; want.s = 2; next.c = 9;
    /* Equal members, different padding: the byte comparison fails and the
     * observed bytes replace the expected ones. */
    int ok = __atomic_compare_exchange(&cell, &want, &next, 0, 5, 5);
    return ok ? 10 : (cell.c != 1) + (want.c != 1) + (want.s != 2);
#elif CASE == 2
    static struct padded cell;
    struct padded put, got, old;
    put.c = 1; put.s = 2;
    __atomic_store(&cell, &put, 5);
    __atomic_load(&cell, &got, 5);
    __atomic_exchange(&cell, &got, &old, 5);
    return got.c + got.s + old.c + old.s != 6;
#else
#error unsupported CASE
#endif
}
