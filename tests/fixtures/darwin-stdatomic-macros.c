/* L11a on Darwin: every macro and generic function of the target's
 * <stdatomic.h>, which is Clang's resource header there. Build with
 * -DCASE=n; exit 0 means that every value checked in that case is the C11
 * value. Apple clang's build must exit 0 as well. One thread runs, so a pass
 * says nothing about inter-thread ordering.
 *
 * CASE 0  lock-free macros, atomic_is_lock_free, layouts, memory_order
 * CASE 1  init, load, store, exchange and compare-exchange of each integer type
 * CASE 2  fetch-and-modify of each integer type, wrapping at its width
 * CASE 3  pointers: element-scaled fetch_add and fetch_sub, exchange, CAS
 * CASE 4  floating objects: load, store, exchange and a bitwise CAS
 * CASE 5  atomic_flag
 * CASE 6  volatile and const objects, kill_dependency, ATOMIC_VAR_INIT
 * CASE 7  relaxed fences, which have no effect
 * CASE 8  ordered fences, which the interpreters refuse (E3021) */
#include <stdatomic.h>
#include <stddef.h>
#include <stdint.h>

#define CHECK(condition) do { if (!(condition)) return 1 + __LINE__ % 255; } while (0)

/* Every integer typedef of 7.17.6 except atomic_bool, with its direct type. */
#define INTEGERS(X) \
    X(atomic_char, char) X(atomic_schar, signed char) X(atomic_uchar, unsigned char) \
    X(atomic_short, short) X(atomic_ushort, unsigned short) X(atomic_int, int) \
    X(atomic_uint, unsigned) X(atomic_long, long) X(atomic_ulong, unsigned long) \
    X(atomic_llong, long long) X(atomic_ullong, unsigned long long) \
    X(atomic_char16_t, uint_least16_t) X(atomic_char32_t, uint_least32_t) \
    X(atomic_wchar_t, wchar_t) X(atomic_int_least8_t, int_least8_t) \
    X(atomic_uint_least8_t, uint_least8_t) X(atomic_int_least16_t, int_least16_t) \
    X(atomic_uint_least16_t, uint_least16_t) X(atomic_int_least32_t, int_least32_t) \
    X(atomic_uint_least32_t, uint_least32_t) X(atomic_int_least64_t, int_least64_t) \
    X(atomic_uint_least64_t, uint_least64_t) X(atomic_int_fast8_t, int_fast8_t) \
    X(atomic_uint_fast8_t, uint_fast8_t) X(atomic_int_fast16_t, int_fast16_t) \
    X(atomic_uint_fast16_t, uint_fast16_t) X(atomic_int_fast32_t, int_fast32_t) \
    X(atomic_uint_fast32_t, uint_fast32_t) X(atomic_int_fast64_t, int_fast64_t) \
    X(atomic_uint_fast64_t, uint_fast64_t) X(atomic_intptr_t, intptr_t) \
    X(atomic_uintptr_t, uintptr_t) X(atomic_size_t, size_t) \
    X(atomic_ptrdiff_t, ptrdiff_t) X(atomic_intmax_t, intmax_t) \
    X(atomic_uintmax_t, uintmax_t)

#if CASE == 0
#if ATOMIC_BOOL_LOCK_FREE != 2 || ATOMIC_CHAR_LOCK_FREE != 2 || ATOMIC_SHORT_LOCK_FREE != 2 \
    || ATOMIC_INT_LOCK_FREE != 2 || ATOMIC_LONG_LOCK_FREE != 2 || ATOMIC_LLONG_LOCK_FREE != 2 \
    || ATOMIC_POINTER_LOCK_FREE != 2 || ATOMIC_CHAR16_T_LOCK_FREE != 2 \
    || ATOMIC_CHAR32_T_LOCK_FREE != 2 || ATOMIC_WCHAR_T_LOCK_FREE != 2
#error lock-free macros
#endif
/* The layout is the direct type's for these sizes (7.17.6p1). */
#define LAYOUT(A, T) _Static_assert(sizeof(A) == sizeof(T) && _Alignof(A) == sizeof(T), #A);
INTEGERS(LAYOUT)
_Static_assert(sizeof(atomic_bool) == 1 && sizeof(atomic_flag) == 1, "bool and flag");
_Static_assert(__c11_atomic_is_lock_free(1) && __c11_atomic_is_lock_free(2)
                   && __c11_atomic_is_lock_free(4) && __c11_atomic_is_lock_free(8),
               "lock-free sizes");
_Static_assert(__c11_atomic_is_lock_free(16), "Apple clang answers 1 for 16 bytes");
_Static_assert(_Generic(__c11_atomic_is_lock_free(4), _Bool: 1, default: 0), "a _Bool");
int main(void)
{
#define LOCK_FREE(A, T) { A object; CHECK(atomic_is_lock_free(&object)); }
    INTEGERS(LOCK_FREE)
    atomic_bool b;
    atomic_flag f;
    _Atomic(int *) p;
    _Atomic(double) d;
    CHECK(atomic_is_lock_free(&b) && atomic_is_lock_free(&f._Value));
    CHECK(atomic_is_lock_free(&p) && atomic_is_lock_free(&d));
    CHECK(memory_order_relaxed == 0 && memory_order_consume == 1 && memory_order_acquire == 2
          && memory_order_release == 3 && memory_order_acq_rel == 4
          && memory_order_seq_cst == 5);
    CHECK(sizeof(memory_order) == sizeof(int));
    return 0;
}

#elif CASE == 1
/* One integer type through init, load, store, exchange and both
 * compare-exchange forms, with every order of the explicit forms. */
#define VALUES(A, T) \
    static int values_##A(void) \
    { \
        A a; \
        atomic_init(&a, (T)2); \
        CHECK(atomic_load(&a) == (T)2); \
        atomic_store(&a, (T)3); \
        CHECK(atomic_load_explicit(&a, memory_order_relaxed) == (T)3); \
        CHECK(atomic_load_explicit(&a, memory_order_consume) == (T)3); \
        CHECK(atomic_load_explicit(&a, memory_order_acquire) == (T)3); \
        CHECK(atomic_load_explicit(&a, memory_order_seq_cst) == (T)3); \
        atomic_store_explicit(&a, (T)4, memory_order_relaxed); \
        atomic_store_explicit(&a, (T)5, memory_order_release); \
        atomic_store_explicit(&a, (T)6, memory_order_seq_cst); \
        CHECK(atomic_exchange(&a, (T)7) == (T)6); \
        CHECK(atomic_exchange_explicit(&a, (T)8, memory_order_relaxed) == (T)7); \
        CHECK(atomic_exchange_explicit(&a, (T)9, memory_order_acquire) == (T)8); \
        CHECK(atomic_exchange_explicit(&a, (T)10, memory_order_release) == (T)9); \
        CHECK(atomic_exchange_explicit(&a, (T)11, memory_order_acq_rel) == (T)10); \
        CHECK(atomic_exchange_explicit(&a, (T)12, memory_order_seq_cst) == (T)11); \
        T e = 12; \
        CHECK(atomic_compare_exchange_strong(&a, &e, (T)13) && e == (T)12); \
        CHECK(!atomic_compare_exchange_strong(&a, &e, (T)14) && e == (T)13); \
        CHECK(atomic_load(&a) == (T)13); \
        while (!atomic_compare_exchange_weak(&a, &e, (T)15)) \
            CHECK(e == (T)13); \
        CHECK(!atomic_compare_exchange_weak(&a, &e, (T)16) && e == (T)15); \
        CHECK(atomic_compare_exchange_strong_explicit(&a, &e, (T)17, memory_order_acq_rel, \
                                                      memory_order_acquire)); \
        CHECK(atomic_compare_exchange_strong_explicit(&a, &e, (T)18, memory_order_relaxed, \
                                                      memory_order_relaxed) == 0 && e == (T)17); \
        /* Apple clang admits a failure order stronger than the success order. */ \
        CHECK(atomic_compare_exchange_strong_explicit(&a, &e, (T)19, memory_order_relaxed, \
                                                      memory_order_seq_cst)); \
        e = 19; \
        while (!atomic_compare_exchange_weak_explicit(&a, &e, (T)20, memory_order_release, \
                                                      memory_order_consume)) \
            CHECK(e == (T)19); \
        CHECK(atomic_load(&a) == (T)20); \
        return 0; \
    }
INTEGERS(VALUES)
static int values_bool(void)
{
    atomic_bool b;
    atomic_init(&b, 0);
    CHECK(!atomic_load(&b));
    atomic_store(&b, 2);
    CHECK(atomic_load_explicit(&b, memory_order_acquire) == 1);
    CHECK(atomic_exchange(&b, 0) == 1);
    _Bool e = 0;
    CHECK(atomic_compare_exchange_strong(&b, &e, 1) && atomic_load(&b));
    CHECK(!atomic_compare_exchange_weak_explicit(&b, &e, 1, memory_order_seq_cst,
                                                 memory_order_relaxed) && e == 1);
    return 0;
}
int main(void)
{
    int failed = values_bool();
#define RUN_VALUES(A, T) if (!failed) failed = values_##A();
    INTEGERS(RUN_VALUES)
    return failed;
}

#elif CASE == 2
/* The fetch-and-modify forms return the old value and wrap at the width of
 * the type (7.17.7.5p3), with every order of the explicit forms. */
#define ARITHMETIC(A, T) \
    static int arithmetic_##A(void) \
    { \
        A a = (T)0x40; \
        CHECK(atomic_fetch_add(&a, 3) == (T)0x40); \
        CHECK(atomic_fetch_sub(&a, 1) == (T)0x43); \
        CHECK(atomic_fetch_or(&a, 0x0c) == (T)0x42); \
        CHECK(atomic_fetch_xor(&a, 0x0f) == (T)0x4e); \
        CHECK(atomic_fetch_and(&a, 0x31) == (T)0x41); \
        CHECK(atomic_load(&a) == (T)0x01); \
        CHECK(atomic_fetch_add_explicit(&a, 2, memory_order_relaxed) == (T)1); \
        CHECK(atomic_fetch_add_explicit(&a, 2, memory_order_consume) == (T)3); \
        CHECK(atomic_fetch_sub_explicit(&a, 1, memory_order_acquire) == (T)5); \
        CHECK(atomic_fetch_or_explicit(&a, 0x10, memory_order_release) == (T)4); \
        CHECK(atomic_fetch_xor_explicit(&a, 0x14, memory_order_acq_rel) == (T)0x14); \
        CHECK(atomic_fetch_and_explicit(&a, 0, memory_order_seq_cst) == (T)0); \
        /* Wrap below zero and above the maximum of the type. */ \
        CHECK(atomic_fetch_sub(&a, 1) == (T)0 && atomic_load(&a) == (T)-1); \
        CHECK(atomic_fetch_add(&a, 1) == (T)-1 && atomic_load(&a) == (T)0); \
        /* The operand converts to the type of the object first. */ \
        atomic_store(&a, (T)5); \
        CHECK(atomic_fetch_add(&a, (T)300) == (T)5 && atomic_load(&a) == (T)(5 + (T)300)); \
        return 0; \
    }
INTEGERS(ARITHMETIC)
static int signed_wrap(void)
{
    atomic_int i = 0x7fffffff;
    CHECK(atomic_fetch_add(&i, 1) == 0x7fffffff && atomic_load(&i) == (int)0x80000000);
    CHECK(atomic_fetch_sub(&i, 1) == (int)0x80000000 && atomic_load(&i) == 0x7fffffff);
    atomic_schar c = 127;
    CHECK(atomic_fetch_add(&c, 1) == 127 && atomic_load(&c) == -128);
    atomic_llong l = 0x7fffffffffffffffLL;
    CHECK(atomic_fetch_add(&l, 2) == 0x7fffffffffffffffLL
          && atomic_load(&l) == (long long)0x8000000000000001ULL);
    return 0;
}
int main(void)
{
    int failed = signed_wrap();
#define RUN_ARITHMETIC(A, T) if (!failed) failed = arithmetic_##A();
    INTEGERS(RUN_ARITHMETIC)
    return failed;
}

#elif CASE == 3
struct big { char bytes[24]; };
static int cells[8];
static struct big bigs[4];
static int one(void) { return 1; }
static int two(void) { return 2; }
int main(void)
{
    /* fetch_add and fetch_sub move a pointer by whole elements. */
    _Atomic(int *) p = cells + 4;
    CHECK(atomic_fetch_add(&p, 2) == cells + 4 && atomic_load(&p) == cells + 6);
    CHECK(atomic_fetch_sub(&p, 3) == cells + 6 && atomic_load(&p) == cells + 3);
    CHECK(atomic_fetch_add_explicit(&p, -1, memory_order_acq_rel) == cells + 3);
    CHECK(atomic_fetch_sub_explicit(&p, -4, memory_order_relaxed) == cells + 2);
    CHECK(atomic_load(&p) == cells + 6);
    _Atomic(struct big *) q = bigs + 1;
    CHECK(atomic_fetch_add(&q, 2) == bigs + 1 && atomic_load(&q) == bigs + 3);
    CHECK((char *)atomic_load(&q) - (char *)bigs == 72);
    _Atomic(char *) s = (char *)cells;
    CHECK(atomic_fetch_add(&s, 5) == (char *)cells && atomic_load(&s) == (char *)cells + 5);
    /* Exchange and compare-exchange keep the pointer itself. */
    CHECK(atomic_exchange(&p, cells) == cells + 6 && *atomic_load(&p) == 0);
    int *e = cells + 1;
    CHECK(!atomic_compare_exchange_strong(&p, &e, cells + 7) && e == cells);
    CHECK(atomic_compare_exchange_strong(&p, &e, cells + 7) && atomic_load(&p) == cells + 7);
    atomic_store_explicit(&p, 0, memory_order_release);
    CHECK(atomic_load(&p) == 0);
    /* A function pointer loads, stores and exchanges. */
    _Atomic(int (*)(void)) f;
    atomic_init(&f, one);
    CHECK(atomic_load(&f)() == 1);
    CHECK(atomic_exchange(&f, two) == one && atomic_load(&f)() == 2);
    int (*g)(void) = two;
    CHECK(atomic_compare_exchange_strong(&f, &g, one) && atomic_load(&f) == one);
    /* intptr_t and uintptr_t objects are integers. */
    atomic_intptr_t ip = 8;
    CHECK(atomic_fetch_or(&ip, 1) == 8 && atomic_load(&ip) == 9);
    atomic_uintptr_t up = (uintptr_t)cells;
    CHECK(atomic_fetch_add(&up, 4) == (uintptr_t)cells && atomic_load(&up) == (uintptr_t)(cells + 1));
    return 0;
}

#elif CASE == 4
int main(void)
{
    _Atomic(float) f;
    atomic_init(&f, 1.5f);
    CHECK(atomic_load(&f) == 1.5f);
    atomic_store_explicit(&f, -2.25f, memory_order_release);
    CHECK(atomic_load_explicit(&f, memory_order_acquire) == -2.25f);
    CHECK(atomic_exchange(&f, 3.0f) == -2.25f && atomic_load(&f) == 3.0f);
    float fe = 3.0f;
    CHECK(atomic_compare_exchange_strong(&f, &fe, 4.5f) && atomic_load(&f) == 4.5f);
    CHECK(!atomic_compare_exchange_weak(&f, &fe, 5.0f) && fe == 4.5f);
    _Atomic(double) d = 0.5;
    atomic_store(&d, 1.0 / 3.0);
    CHECK(atomic_load(&d) == 1.0 / 3.0);
    CHECK(atomic_exchange_explicit(&d, 2.0, memory_order_acq_rel) == 1.0 / 3.0);
    /* The comparison is the one of the object representations: -0.0 and
     * 0.0 differ, and a NaN equals the same NaN. */
    atomic_store(&d, -0.0);
    double de = 0.0;
    CHECK(!atomic_compare_exchange_strong(&d, &de, 1.0) && de == 0.0 && 1.0 / de < 0);
    CHECK(atomic_compare_exchange_strong(&d, &de, 1.0) && atomic_load(&d) == 1.0);
    atomic_store(&d, __builtin_nan(""));
    de = __builtin_nan("");
    CHECK(atomic_compare_exchange_strong(&d, &de, 6.0) && atomic_load(&d) == 6.0);
    /* An integer operand converts to the floating type. */
    atomic_store(&d, 7);
    CHECK(atomic_load(&d) == 7.0);
    /* long double is binary64 on Darwin. */
    _Atomic(long double) q = 3.0L;
    CHECK(sizeof q == 8 && atomic_exchange(&q, 4.0L) == 3.0L && atomic_load(&q) == 4.0L);
    return 0;
}

#elif CASE == 5
static atomic_flag shared = ATOMIC_FLAG_INIT;
int main(void)
{
    CHECK(!atomic_flag_test_and_set(&shared));
    CHECK(atomic_flag_test_and_set(&shared));
    atomic_flag_clear(&shared);
    CHECK(!atomic_flag_test_and_set_explicit(&shared, memory_order_acquire));
    CHECK(atomic_flag_test_and_set_explicit(&shared, memory_order_relaxed));
    atomic_flag_clear_explicit(&shared, memory_order_release);
    CHECK(!atomic_flag_test_and_set_explicit(&shared, memory_order_seq_cst));
    atomic_flag_clear_explicit(&shared, memory_order_relaxed);
    atomic_flag local = ATOMIC_FLAG_INIT;
    volatile atomic_flag *v = &local;
    CHECK(!atomic_flag_test_and_set_explicit(v, memory_order_acq_rel));
    CHECK(atomic_flag_test_and_set(v));
    atomic_flag_clear_explicit(v, memory_order_seq_cst);
    CHECK(!atomic_flag_test_and_set(&local));
    return 0;
}

#elif CASE == 6
static const atomic_int fixed = ATOMIC_VAR_INIT(41);
static volatile atomic_long counter;
int main(void)
{
    CHECK(atomic_load(&fixed) == 41);
    CHECK(atomic_load_explicit(&fixed, memory_order_acquire) == 41);
    CHECK(kill_dependency(atomic_load(&fixed)) + 1 == 42);
    volatile atomic_long *c = &counter;
    atomic_init(c, 5);
    CHECK(atomic_fetch_add(c, 2) == 5 && atomic_load(c) == 7);
    CHECK(atomic_exchange_explicit(c, 9, memory_order_release) == 7);
    long e = 9;
    CHECK(atomic_compare_exchange_strong(c, &e, 10) && atomic_load(c) == 10);
    atomic_store(c, 0);
    atomic_ushort u = ATOMIC_VAR_INIT(65535);
    CHECK(atomic_fetch_add(&u, 1) == 65535 && atomic_load(&u) == 0);
    return 0;
}

#elif CASE == 7
int main(void)
{
    atomic_int a = 1;
    atomic_thread_fence(memory_order_relaxed);
    atomic_signal_fence(memory_order_relaxed);
    CHECK(atomic_load_explicit(&a, memory_order_relaxed) == 1);
    return 0;
}

#elif CASE == 8
int main(void)
{
    atomic_int a = 1;
    atomic_thread_fence(memory_order_consume);
    atomic_thread_fence(memory_order_acquire);
    atomic_thread_fence(memory_order_release);
    atomic_thread_fence(memory_order_acq_rel);
    atomic_thread_fence(memory_order_seq_cst);
    atomic_signal_fence(memory_order_acquire);
    atomic_signal_fence(memory_order_seq_cst);
    CHECK(atomic_load(&a) == 1);
    return 0;
}
#endif
