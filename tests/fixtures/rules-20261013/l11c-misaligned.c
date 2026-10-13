/* L11c: observation probe, references only. An 8-byte record whose type
 * alignment is 4 sits at an address that is 4 modulo 8. This alignment is
 * valid for the type. CASE=0 load/store, CASE=1 compare-exchange. */
#include <stdlib.h>
#ifndef CASE
#define CASE 0
#endif
struct four { int a, b; };

int main(void)
{
    char *block = malloc(32);
    if (!block || ((unsigned long)block & 15)) return 99;
    struct four *p = (struct four *)(block + 4);
    struct four v = {1, 2}, r = {0, 0};
#if CASE == 0
    __atomic_store(p, &v, 5);
    __atomic_load(p, &r, 5);
    return r.b != 2;
#else
    struct four want = {0, 0};
    p->a = 0; p->b = 0;
    if (!__atomic_compare_exchange(p, &want, &v, 0, 5, 5)) return 2;
    return p->b != 2;
#endif
}
