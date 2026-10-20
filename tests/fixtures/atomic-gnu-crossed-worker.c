/* L11b: one side of the crossed GNU __atomic test. Build it once per
 * compiler with -DSIDE=name. The driver runs two sides in two threads on the
 * same objects, so each object is updated by code from two compilers. A pass
 * shows no lost update and no forbidden outcome in these runs on this
 * machine; it is not a memory-model proof. */
#define CAT2(a, b) a##_##b
#define CAT(a, b) CAT2(a, b)
#define NAME(n) CAT(SIDE, n)

#define WIDTHS(X) \
    X(u8, unsigned char) X(u16, unsigned short) X(u32, unsigned) X(u64, unsigned long)

/* Arithmetic and bitwise forms, one function per width and order. */
#define ARITH(tag, T, o) \
    void NAME(fetch_add_##tag##_##o)(T *p, int n) \
    { for (int i = 0; i < n; i++) __atomic_fetch_add(p, 1, o); } \
    void NAME(add_fetch_##tag##_##o)(T *p, int n) \
    { for (int i = 0; i < n; i++) __atomic_add_fetch(p, 1, o); } \
    void NAME(fetch_sub_##tag##_##o)(T *p, int n) \
    { for (int i = 0; i < n; i++) __atomic_fetch_sub(p, 1, o); } \
    void NAME(sub_fetch_##tag##_##o)(T *p, int n) \
    { for (int i = 0; i < n; i++) __atomic_sub_fetch(p, 1, o); } \
    void NAME(xor_##tag##_##o)(T *p, int n, T mask) \
    { for (int i = 0; i < n; i++) { __atomic_fetch_xor(p, mask, o); __atomic_xor_fetch(p, mask, o); } } \
    void NAME(nand_##tag##_##o)(T *p, int n) \
    { for (int i = 0; i < n; i++) { __atomic_fetch_nand(p, (T)~(T)0, o); __atomic_nand_fetch(p, (T)~(T)0, o); } } \
    int NAME(or_and_##tag##_##o)(T *p, int n, T bit) \
    { \
        int lost = 0; \
        for (int i = 0; i < n; i++) { \
            lost += !(__atomic_or_fetch(p, bit, o) & bit); \
            lost += !(__atomic_fetch_and(p, (T)~bit, o) & bit); \
            lost += !!(__atomic_fetch_or(p, bit, o) & bit); \
            lost += !!(__atomic_and_fetch(p, (T)~bit, o) & bit); \
        } \
        return lost; \
    }
#define ARITH_ORDERS(tag, T) \
    ARITH(tag, T, 0) ARITH(tag, T, 1) ARITH(tag, T, 2) \
    ARITH(tag, T, 3) ARITH(tag, T, 4) ARITH(tag, T, 5)
WIDTHS(ARITH_ORDERS)

/* Exchange as a lock word of each width; the count is a plain object. */
#define LOCK(tag, T, o) \
    void NAME(exchange_lock_##tag##_##o)(T *lock, long *count, int n) \
    { \
        for (int i = 0; i < n; i++) { \
            while (__atomic_exchange_n(lock, 1, o)) { } \
            ++*count; \
            __atomic_store_n(lock, 0, 3); \
        } \
    }
#define LOCK_ORDERS(tag, T) LOCK(tag, T, 2) LOCK(tag, T, 4) LOCK(tag, T, 5)
WIDTHS(LOCK_ORDERS)

/* Strong and weak compare-exchange loops for success/failure pairs. */
#define CAS(tag, T, s, f) \
    void NAME(cas_weak_##tag##_##s##f)(T *p, int n) \
    { \
        for (int i = 0; i < n; i++) { \
            T e = __atomic_load_n(p, 0); \
            while (!__atomic_compare_exchange_n(p, &e, (T)(e + 1), 1, s, f)) { } \
        } \
    } \
    void NAME(cas_strong_##tag##_##s##f)(T *p, int n) \
    { \
        for (int i = 0; i < n; i++) { \
            T e = __atomic_load_n(p, 0); \
            while (!__atomic_compare_exchange_n(p, &e, (T)(e + 1), 0, s, f)) { } \
        } \
    }
#define CAS_PAIRS(tag, T) \
    CAS(tag, T, 5, 5) CAS(tag, T, 4, 2) CAS(tag, T, 3, 0) \
    CAS(tag, T, 2, 1) CAS(tag, T, 1, 1) CAS(tag, T, 0, 0)
WIDTHS(CAS_PAIRS)

/* Test-and-set and clear as a lock. */
#define FLAG(s, c) \
    void NAME(flag_lock_##s##c)(_Bool *lock, long *count, int n) \
    { \
        for (int i = 0; i < n; i++) { \
            while (__atomic_test_and_set(lock, s)) { } \
            ++*count; \
            __atomic_clear(lock, c); \
        } \
    }
FLAG(2, 3) FLAG(4, 3) FLAG(5, 5)

/* Pointer forms add bytes, unscaled. */
#define POINTER(o) \
    void NAME(pointer_##o)(int **p, int n) \
    { \
        for (int i = 0; i < n; i++) { \
            __atomic_fetch_add(p, 5, o); \
            __atomic_sub_fetch(p, 2, o); \
        } \
    }
POINTER(0) POINTER(2) POINTER(3) POINTER(5)

/* Fences: message passing through relaxed accesses, and store buffering. */
void NAME(publish)(int *data, int *flag)
{
    __atomic_store_n(data, 42, 0);
    __atomic_thread_fence(3);
    __atomic_store_n(flag, 1, 0);
}

int NAME(receive)(int *data, int *flag)
{
    while (!__atomic_load_n(flag, 0)) { }
    __atomic_thread_fence(2);
    return __atomic_load_n(data, 0);
}

int NAME(store_buffer)(int *mine, int *other)
{
    __atomic_store_n(mine, 1, 0);
    __atomic_signal_fence(5);
    __atomic_thread_fence(5);
    return __atomic_load_n(other, 0);
}
