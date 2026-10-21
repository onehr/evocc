/* L11a on Darwin: one side of the crossed <stdatomic.h> test. Build it once
 * per compiler with -DSIDE=name. The driver runs two sides in two threads on
 * the same _Atomic objects, so each object is updated by code from two
 * compilers through the header's generic functions. A pass shows no lost
 * update and no forbidden outcome in these runs on this machine; it is not a
 * memory-model proof. */
#include <stdatomic.h>

#define CAT2(a, b) a##_##b
#define CAT(a, b) CAT2(a, b)
#define NAME(n) CAT(SIDE, n)

#define WIDTHS(X) \
    X(u8, atomic_uchar, unsigned char) X(u16, atomic_ushort, unsigned short) \
    X(u32, atomic_uint, unsigned) X(u64, atomic_ulong, unsigned long)

/* The fetch-and-modify forms, one function per width and order. */
#define ARITH(tag, A, T, o) \
    void NAME(fetch_add_##tag##_##o)(A *p, int n) \
    { for (int i = 0; i < n; i++) atomic_fetch_add_explicit(p, 1, o); } \
    void NAME(fetch_sub_##tag##_##o)(A *p, int n) \
    { for (int i = 0; i < n; i++) atomic_fetch_sub_explicit(p, 1, o); } \
    void NAME(xor_##tag##_##o)(A *p, int n, T mask) \
    { for (int i = 0; i < n; i++) { atomic_fetch_xor_explicit(p, mask, o); atomic_fetch_xor(p, mask); } } \
    int NAME(or_and_##tag##_##o)(A *p, int n, T bit) \
    { \
        int lost = 0; \
        for (int i = 0; i < n; i++) { \
            lost += !!(atomic_fetch_or_explicit(p, bit, o) & bit); \
            lost += !(atomic_fetch_and_explicit(p, (T)~bit, o) & bit); \
        } \
        return lost; \
    }
#define ARITH_ORDERS(tag, A, T) \
    ARITH(tag, A, T, 0) ARITH(tag, A, T, 1) ARITH(tag, A, T, 2) \
    ARITH(tag, A, T, 3) ARITH(tag, A, T, 4) ARITH(tag, A, T, 5)
WIDTHS(ARITH_ORDERS)

/* Exchange as a lock word of each width; the count is a plain object. */
#define LOCK(tag, A, T, o) \
    void NAME(exchange_lock_##tag##_##o)(A *lock, long *count, int n) \
    { \
        for (int i = 0; i < n; i++) { \
            while (atomic_exchange_explicit(lock, 1, o)) { } \
            ++*count; \
            atomic_store_explicit(lock, 0, memory_order_release); \
        } \
    }
#define LOCK_ORDERS(tag, A, T) LOCK(tag, A, T, 2) LOCK(tag, A, T, 4) LOCK(tag, A, T, 5)
WIDTHS(LOCK_ORDERS)

/* Strong and weak compare-exchange loops for success/failure pairs. The
 * pair 0/5 has a failure order stronger than its success order, which Apple
 * clang admits. */
#define CAS(tag, A, T, s, f) \
    void NAME(cas_weak_##tag##_##s##f)(A *p, int n) \
    { \
        for (int i = 0; i < n; i++) { \
            T e = atomic_load_explicit(p, memory_order_relaxed); \
            while (!atomic_compare_exchange_weak_explicit(p, &e, (T)(e + 1), s, f)) { } \
        } \
    } \
    void NAME(cas_strong_##tag##_##s##f)(A *p, int n) \
    { \
        for (int i = 0; i < n; i++) { \
            T e = atomic_load(p); \
            while (!atomic_compare_exchange_strong_explicit(p, &e, (T)(e + 1), s, f)) { } \
        } \
    }
#define CAS_PAIRS(tag, A, T) \
    CAS(tag, A, T, 5, 5) CAS(tag, A, T, 4, 2) CAS(tag, A, T, 3, 0) \
    CAS(tag, A, T, 2, 1) CAS(tag, A, T, 0, 0) CAS(tag, A, T, 0, 5)
WIDTHS(CAS_PAIRS)

/* atomic_flag as a lock. */
#define FLAG(s, c) \
    void NAME(flag_lock_##s##c)(atomic_flag *lock, long *count, int n) \
    { \
        for (int i = 0; i < n; i++) { \
            while (atomic_flag_test_and_set_explicit(lock, s)) { } \
            ++*count; \
            atomic_flag_clear_explicit(lock, c); \
        } \
    }
FLAG(2, 3) FLAG(4, 3) FLAG(5, 5)

/* Pointer forms move by whole elements. */
#define POINTER(o) \
    void NAME(pointer_##o)(_Atomic(int *) *p, int n) \
    { \
        for (int i = 0; i < n; i++) { \
            atomic_fetch_add_explicit(p, 5, o); \
            atomic_fetch_sub_explicit(p, 2, o); \
        } \
    }
POINTER(0) POINTER(2) POINTER(3) POINTER(5)

/* A floating counter through a compare-exchange loop. */
void NAME(double_add)(_Atomic(double) *p, int n)
{
    for (int i = 0; i < n; i++) {
        double e = atomic_load_explicit(p, memory_order_relaxed);
        while (!atomic_compare_exchange_weak(p, &e, e + 1.0)) { }
    }
}

/* Fences: message passing through relaxed accesses, and store buffering. */
void NAME(publish)(atomic_int *data, atomic_int *flag)
{
    atomic_store_explicit(data, 42, memory_order_relaxed);
    atomic_thread_fence(memory_order_release);
    atomic_store_explicit(flag, 1, memory_order_relaxed);
}

int NAME(receive)(atomic_int *data, atomic_int *flag)
{
    while (!atomic_load_explicit(flag, memory_order_relaxed)) { }
    atomic_thread_fence(memory_order_acquire);
    return atomic_load_explicit(data, memory_order_relaxed);
}

int NAME(store_buffer)(atomic_int *mine, atomic_int *other)
{
    atomic_store_explicit(mine, 1, memory_order_relaxed);
    atomic_signal_fence(memory_order_seq_cst);
    atomic_thread_fence(memory_order_seq_cst);
    return atomic_load_explicit(other, memory_order_relaxed);
}
