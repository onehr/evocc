/* L11a: one side of the crossed _Atomic object test. Build it once per
 * compiler with -DSIDE=name. The driver runs two sides in two threads on one
 * struct holder, so code from two compilers updates each atomic member
 * through C operators. A pass shows no lost update and no torn value in
 * these runs on this machine; it is not a memory-model proof. */
#include "atomic-objects-crossed.h"

#define NAME(n) CAT(SIDE, n)

void NAME(layout)(long *out)
{
    LAYOUT(out);
}

/* Each iteration adds one to counter, wide and the cursor element index,
 * multiplies product by 3 and adds one to small and wrap. Clang rejects
 * `+=` on an atomic pointer, which C11 6.5.16.2p1 allows, so the cursor
 * uses `++` and `--` only. */
void NAME(count)(struct holder *h, int n)
{
    for (int i = 0; i < n; i++) {
        h->counter += 3;
        h->counter -= 2;
        h->counter++;
        --h->counter;
        h->product *= 3;
        h->small++;
        ++h->wrap;
        h->wide += 2;
        h->wide--;
        h->cursor++;
        ++h->cursor;
        h->cursor--;
        h->flag = 1;
    }
}

/* Single-thread values of the operators. `_Bool` compound assignment is
 * left out: Clang 19 and Apple clang store 2 after `b += 1` from 1. */
int NAME(values)(void)
{
    _Atomic unsigned x = 3;
    _Atomic _Bool b = 0;
    _Atomic signed char c = 127;
    int *_Atomic p = &cells[0];
    if ((x += 4) != 7 || x++ != 7 || ++x != 9 || x-- != 9 || --x != 7) return 1;
    if ((x *= 3) != 21 || (x /= 2) != 10 || (x %= 4) != 2 || (x <<= 3) != 16) return 2;
    if ((x >>= 2) != 4 || (x |= 9) != 13 || (x &= 6) != 4 || (x ^= 5) != 1) return 3;
    b = 5;
    if (b != 1) return 4;
    b = 0;
    if (b != 0) return 5;
    if (++c != -128) return 6;
    p++;
    ++p;
    p++;
    --p;
    return p != &cells[2] ? 7 : 0;
}
