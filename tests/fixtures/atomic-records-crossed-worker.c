/* L11c: one side of the crossed generic-atomic record test. Build it once
 * per compiler with -DSIDE=name. The driver runs two sides in two threads on
 * the same records, so code from two compilers updates each record with
 * __atomic_load, __atomic_store, __atomic_exchange and
 * __atomic_compare_exchange. A pass shows no lost update and no torn record
 * in these runs on this machine; it is not a memory-model proof. */
#include "atomic-records-crossed.h"

#define NAME(n) CAT(SIDE, n)

void NAME(work)(int n, int me)
{
    for (int i = 0; i < n; i++) {
        struct w8 o8, n8;
        __atomic_load(&c8, &o8, 0);
        do { n8.count = o8.count + 1; n8.check = o8.check + 2; }
        while (!__atomic_compare_exchange(&c8, &o8, &n8, 1, 5, 0));
        struct w4 o4, n4;
        __atomic_load(&c4, &o4, 2);
        do { n4 = o4; n4.count++; n4.check += 2; }
        while (!__atomic_compare_exchange(&c4, &o4, &n4, 0, 4, 2));
        struct w2 o2, n2;
        __atomic_load(&c2, &o2, 5);
        do { n2.count = (unsigned char)(o2.count + 1); n2.check = (unsigned char)(o2.check + 2); }
        while (!__atomic_compare_exchange(&c2, &o2, &n2, 1, 3, 0));
        struct w1 o1, n1;
        __atomic_load(&c1, &o1, 0);
        do { n1.count = (unsigned char)(o1.count + 1); }
        while (!__atomic_compare_exchange(&c1, &o1, &n1, 0, 5, 5));
#ifdef __clang__
        /* Clang refuses the GNU forms on an _Atomic object; its C11 forms
         * update the same object. */
        struct pair op = __c11_atomic_load(&cp, 5), np;
        do { np.count = op.count + 1; np.check = op.check + 2; }
        while (!__c11_atomic_compare_exchange_strong(&cp, &op, np, 5, 5));
#else
        struct pair op, np;
        __atomic_load(&cp, &op, 5);
        do { np.count = op.count + 1; np.check = op.check + 2; }
        while (!__atomic_compare_exchange(&cp, &op, &np, 0, 5, 5));
#endif
        struct lock want = {1, me}, previous;
        do __atomic_exchange(&guard, &want, &previous, 2);
        while (previous.held);
        guarded++;
        struct lock open = {0, me};
        __atomic_store(&guard, &open, 3);
    }
}
