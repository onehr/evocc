/* L11c: generic __atomic_load/store/exchange/compare_exchange on records.
 * Single-thread value probe, CASE=0..5; exit 0 is the asserted answer.
 * Every record is fully initialized, padding included, before any atomic
 * comparison; CASE=4 observes that padding bytes take part in the compare. */
#include <string.h>
#ifndef CASE
#define CASE 0
#endif
static int first, second;
struct __attribute__((aligned(8))) pair { unsigned a, b; };
struct __attribute__((aligned(16))) tagged { long count; int *pointer; };
struct three { char c[3]; };
struct twelve { int a, b, c; };
struct padded { char c; short s; };

int main(void)
{
#if CASE == 0 /* 8-byte naturally aligned record: all four generic forms */
    static struct pair cell = {3, 7};
    struct pair old, next = {11, 13}, want = {3, 7};
    __atomic_load(&cell, &old, 2);
    if (old.a != 3 || old.b != 7) return 1;
    __atomic_exchange(&cell, &next, &old, 4);
    if (old.a != 3 || cell.a != 11) return 2;
    if (__atomic_compare_exchange(&cell, &want, &next, 0, 5, 5)) return 3;
    if (want.a != 11 || want.b != 13) return 4;
    __atomic_store(&cell, &(struct pair){1, 2}, 3);
    return cell.b != 2;
#elif CASE == 1 /* 16-byte pointer-bearing record */
    struct tagged cell, want, next;
    memset(&cell, 0, sizeof cell); memset(&want, 0, sizeof want);
    memset(&next, 0, sizeof next);
    cell.count = 1; cell.pointer = &first;
    want = cell; next.count = 2; next.pointer = &second;
    if (!__atomic_compare_exchange(&cell, &want, &next, 0, 5, 5)) return 1;
    if (cell.pointer != &second || *cell.pointer != 0) return 2;
    struct tagged seen;
    __atomic_load(&cell, &seen, 5);
    return seen.count != 2 || seen.pointer != &second;
#elif CASE == 2 /* 3-byte record: not a lock-free size */
    static struct three cell = {{1, 2, 3}};
    struct three old, next = {{4, 5, 6}};
    __atomic_exchange(&cell, &next, &old, 5);
    return old.c[2] != 3 || cell.c[0] != 4;
#elif CASE == 3 /* 12-byte record: generic library path */
    static struct twelve cell = {1, 2, 3};
    struct twelve want = {1, 2, 3}, next = {4, 5, 6};
    if (!__atomic_compare_exchange(&cell, &want, &next, 0, 5, 5)) return 1;
    struct twelve seen;
    __atomic_load(&cell, &seen, 5);
    return seen.c != 6;
#elif CASE == 4 /* padding bytes take part: equal members, different padding */
    struct padded cell, want, next;
    memset(&cell, 0, sizeof cell); memset(&want, 0xff, sizeof want);
    memset(&next, 0, sizeof next);
    cell.c = 1; cell.s = 2; want.c = 1; want.s = 2; next.c = 9;
    int ok = __atomic_compare_exchange(&cell, &want, &next, 0, 5, 5);
    return ok ? 10 : (cell.c == 1 ? 0 : 1); /* observation: 0 = byte compare */
#elif CASE == 5 /* lock-free queries for the record sizes above */
    return (__atomic_always_lock_free(sizeof(struct pair), 0) ? 1 : 0)
         | (__atomic_always_lock_free(sizeof(struct tagged), 0) ? 2 : 0)
         | (__atomic_always_lock_free(sizeof(struct three), 0) ? 4 : 0)
         | (__atomic_always_lock_free(sizeof(struct twelve), 0) ? 8 : 0);
#else
#error unsupported CASE
#endif
}
