/* getcontext returns twice when setcontext resumes its context (D16; Linux
   only). getcontext returns 0 both times, so a volatile counter tells the
   returns apart. Fourteen unchanged values live across the call, and the
   path to setcontext keeps fourteen temporaries live across calls. main
   returns 0 when every kept value survives both resumptions. */
#include <ucontext.h>

static ucontext_t context;
volatile int sink;

__attribute__((noinline)) static int opaque(int v) { sink = v; return v * 13 + 5; }

static int resumed(int seed) {
    volatile int count = 0;
    int a0 = opaque(seed), a1 = opaque(seed + 1), a2 = opaque(seed + 2),
        a3 = opaque(seed + 3), a4 = opaque(seed + 4), a5 = opaque(seed + 5),
        a6 = opaque(seed + 6), a7 = opaque(seed + 7), a8 = opaque(seed + 8),
        a9 = opaque(seed + 9), a10 = opaque(seed + 10), a11 = opaque(seed + 11),
        a12 = opaque(seed + 12), a13 = opaque(seed + 13);
    if (getcontext(&context) != 0)
        return -1;
    count++;
    if (count < 3) {
        int t0 = opaque(100), t1 = opaque(101), t2 = opaque(102),
            t3 = opaque(103), t4 = opaque(104), t5 = opaque(105),
            t6 = opaque(106), t7 = opaque(107), t8 = opaque(108),
            t9 = opaque(109), t10 = opaque(110), t11 = opaque(111),
            t12 = opaque(112), t13 = opaque(113);
        sink = t0 + t1 + t2 + t3 + t4 + t5 + t6 + t7 + t8 + t9 + t10 + t11
               + t12 + t13;
        setcontext(&context);
        return -2;
    }
    return a0 + a1 + a2 + a3 + a4 + a5 + a6 + a7 + a8 + a9 + a10 + a11 + a12
           + a13 + count * 100000;
}

int main(void) {
    int expected = 3 * 100000;
    for (int i = 0; i < 14; i++)
        expected += (7 + i) * 13 + 5;
    return resumed(7) == expected ? 0 : 1;
}
