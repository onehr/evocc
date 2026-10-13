/* L11b: GNU __atomic read-modify-write, exchange and compare-exchange.
 * Single-thread value probe; CASE=0..8. Exit 0 is the asserted answer
 * unless the case comment names an observation code. */
#ifndef CASE
#define CASE 0
#endif
static int calls;
static unsigned cell = 5;
static unsigned *where(void) { calls = calls * 10 + 1; return &cell; }
static unsigned what(void) { calls = calls * 10 + 2; return 3; }
static int order(void) { calls = calls * 10 + 3; return 5; }

int main(void)
{
#if CASE == 0 /* every fetch-op/op-fetch form, widths 1/2/4/8, wraparound */
    unsigned char b = 250; signed char s = 127; unsigned short h = 1;
    int i = 6; long l = 1; unsigned long u = 0;
    if (__atomic_fetch_add(&b, 10, 0) != 250 || b != 4) return 1;
    if (__atomic_add_fetch(&s, 1, 5) != -128) return 2;
    if (__atomic_fetch_sub(&h, 2, 3) != 1 || h != 0xffff) return 3;
    if (__atomic_fetch_and(&i, 3, 2) != 6 || i != 2) return 4;
    if (__atomic_or_fetch(&l, 0x100000000L, 4) != 0x100000001L) return 5;
    if (__atomic_xor_fetch(&u, ~0UL, 1) != ~0UL) return 6;
    if (__atomic_nand_fetch(&i, 3, 5) != ~2) return 7;
    if (__atomic_fetch_nand(&b, 1, 5) != 4 || b != 0xff) return 8;
    if (__atomic_sub_fetch(&s, 1, 0) != 127) return 9;
    return 0;
#elif CASE == 1 /* strong and weak CAS: failure writeback, success, orders */
    unsigned v = 3, e = 4;
    if (__atomic_compare_exchange_n(&v, &e, 7, 0, 5, 5)) return 1;
    if (e != 3 || v != 3) return 2;
    if (!__atomic_compare_exchange_n(&v, &e, 7, 0, 4, 2) || v != 7 || e != 3) return 3;
    e = 9;
    if (__atomic_compare_exchange_n(&v, &e, 1, 0, 3, 0) || e != 7) return 4;
    while (!__atomic_compare_exchange_n(&v, &e, e + 1, 1, 0, 0)) { }
    if (v != 8 || e != 7) return 5;
    return 0;
#elif CASE == 2 /* exchange on integer, pointer and _Bool */
    int x = 1, y = 2, *p = &x; _Bool flag = 0; long w = -1;
    if (__atomic_exchange_n(&p, &y, 5) != &x || *p != 2) return 1;
    if (__atomic_exchange_n(&flag, 1, 2) != 0 || !flag) return 2;
    if (__atomic_exchange_n(&w, 5, 0) != -1 || w != 5) return 3;
    return 0;
#elif CASE == 3 /* pointer arithmetic: 0 = unscaled bytes, 10 = scaled */
    int arr[4] = {0}; int *p = arr;
    __atomic_fetch_add(&p, 4, 5);
    if (p == arr + 1) return 0;
    if (p == arr + 4) return 10;
    return 1;
#elif CASE == 4 /* runtime orders are evaluated once, then strengthened */
    volatile int o = 2; unsigned v = 1, e = 1;
    if (__atomic_fetch_add(&v, 1, o) != 1) return 1;
    if (__atomic_compare_exchange_n(&v, &e, 4, 0, o, o) || e != 2) return 2;
    if (__atomic_exchange_n(&v, 9, o) != 2 || v != 9) return 3;
    return 0;
#elif CASE == 5 /* test_and_set, clear and fences */
    _Bool f = 0; unsigned char c = 0;
    if (__atomic_test_and_set(&f, 5) || !__atomic_test_and_set(&f, 2)) return 1;
    __atomic_clear(&f, 3);
    if (f || __atomic_test_and_set(&c, 0)) return 2;
    __atomic_thread_fence(5); __atomic_signal_fence(2);
    return c == 0;
#elif CASE == 6 /* 16-byte integer RMW and CAS (library or inline) */
    unsigned __int128 q = 1, e = 1;
    if (__atomic_fetch_add(&q, (unsigned __int128)1 << 64, 5) != 1) return 1;
    e = q;
    if (!__atomic_compare_exchange_n(&q, &e, 7, 0, 5, 5) || q != 7) return 2;
    return 0;
#elif CASE == 7 /* operand evaluation count; order code records sequence */
    unsigned old = __atomic_fetch_add(where(), what(), order());
    if (old != 5 || cell != 8) return 1;
    return calls == 123 ? 0 : calls % 100; /* observation: evaluation order */
#elif CASE == 8 /* GNU builtins on an _Atomic-qualified object */
    static _Atomic int a = 2; int e = 2;
    if (__atomic_fetch_add(&a, 3, 5) != 2) return 1;
    if (!__atomic_compare_exchange_n(&a, &(int){5}, 6, 0, 5, 5)) return 2;
    if (__atomic_compare_exchange_n(&a, &e, 0, 0, 5, 5) || e != 6) return 3;
    return __atomic_load_n(&a, 5) != 6;
#else
#error unsupported CASE
#endif
}
