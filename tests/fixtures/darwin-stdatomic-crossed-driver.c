/* L11a on Darwin: run two builds of darwin-stdatomic-crossed-worker.c in two
 * threads on the same _Atomic objects. Build with -DFIRST=name
 * -DSECOND=name. Each test starts both threads together; thread one calls
 * FIRST, thread two calls SECOND. The exit status is 0 when every test
 * passes. A pointer to an _Atomic object becomes void * only through a cast,
 * as C11 6.5.16.1 requires. A pass shows no lost update and no forbidden
 * outcome in these runs; it is not a memory-model proof. */
#include <pthread.h>
#include <stdatomic.h>
#include <stdio.h>

#define CAT2(a, b) a##_##b
#define CAT(a, b) CAT2(a, b)
#define N 20000
#define ROUNDS 2000

#define WIDTHS(X) \
    X(u8, atomic_uchar, unsigned char) X(u16, atomic_ushort, unsigned short) \
    X(u32, atomic_uint, unsigned) X(u64, atomic_ulong, unsigned long)

static atomic_int ready;
static int failures, tests;
struct job { void (*run)(void *); void *arg; };

static void *start(void *raw)
{
    struct job *job = raw;
    atomic_fetch_add(&ready, 1);
    while (atomic_load(&ready) < 2) { }
    job->run(job->arg);
    return 0;
}

static void run_pair(void (*first)(void *), void (*second)(void *), void *arg)
{
    pthread_t a, b;
    struct job one = { first, arg }, two = { second, arg };
    atomic_store(&ready, 0);
    if (pthread_create(&a, 0, start, &one) || pthread_create(&b, 0, start, &two)) {
        printf("thread creation failed\n");
        failures++;
        return;
    }
    pthread_join(a, 0);
    pthread_join(b, 0);
}

static void check(const char *name, int ok)
{
    tests++;
    if (!ok) {
        failures++;
        printf("fail %s\n", name);
    }
}

/* The worker's functions, for both sides. */
#define ARITH_DECL(side, tag, A, T, o) \
    void CAT(side, fetch_add_##tag##_##o)(A *, int); \
    void CAT(side, fetch_sub_##tag##_##o)(A *, int); \
    void CAT(side, xor_##tag##_##o)(A *, int, T); \
    int CAT(side, or_and_##tag##_##o)(A *, int, T);
#define LOCK_DECL(side, tag, A, T, o) void CAT(side, exchange_lock_##tag##_##o)(A *, long *, int);
#define CAS_DECL(side, tag, A, T, pair) \
    void CAT(side, cas_weak_##tag##_##pair)(A *, int); \
    void CAT(side, cas_strong_##tag##_##pair)(A *, int);
#define SIDE_DECL(side) \
    void CAT(side, flag_lock_23)(atomic_flag *, long *, int); \
    void CAT(side, flag_lock_43)(atomic_flag *, long *, int); \
    void CAT(side, flag_lock_55)(atomic_flag *, long *, int); \
    void CAT(side, pointer_0)(_Atomic(int *) *, int); \
    void CAT(side, pointer_2)(_Atomic(int *) *, int); \
    void CAT(side, pointer_3)(_Atomic(int *) *, int); \
    void CAT(side, pointer_5)(_Atomic(int *) *, int); \
    void CAT(side, double_add)(_Atomic(double) *, int); \
    void CAT(side, publish)(atomic_int *, atomic_int *); \
    int CAT(side, receive)(atomic_int *, atomic_int *); \
    int CAT(side, store_buffer)(atomic_int *, atomic_int *);
#define ORDERS_DECL(side, tag, A, T) \
    ARITH_DECL(side, tag, A, T, 0) ARITH_DECL(side, tag, A, T, 1) \
    ARITH_DECL(side, tag, A, T, 2) ARITH_DECL(side, tag, A, T, 3) \
    ARITH_DECL(side, tag, A, T, 4) ARITH_DECL(side, tag, A, T, 5) \
    LOCK_DECL(side, tag, A, T, 2) LOCK_DECL(side, tag, A, T, 4) LOCK_DECL(side, tag, A, T, 5) \
    CAS_DECL(side, tag, A, T, 55) CAS_DECL(side, tag, A, T, 42) CAS_DECL(side, tag, A, T, 30) \
    CAS_DECL(side, tag, A, T, 21) CAS_DECL(side, tag, A, T, 00) CAS_DECL(side, tag, A, T, 05)
#define WIDTH_DECL(tag, A, T) ORDERS_DECL(FIRST, tag, A, T) ORDERS_DECL(SECOND, tag, A, T)
WIDTHS(WIDTH_DECL)
SIDE_DECL(FIRST)
SIDE_DECL(SECOND)

/* Arithmetic: counters, xor toggles, and owned bits. */
#define ARITH_TEST(tag, A, T, o) \
    static A shared_##tag##_##o; \
    static int lost_a_##tag##_##o, lost_b_##tag##_##o; \
    static void a_add_##tag##_##o(void *p) { CAT(FIRST, fetch_add_##tag##_##o)(p, N); } \
    static void b_add_##tag##_##o(void *p) { CAT(SECOND, fetch_add_##tag##_##o)(p, N); } \
    static void a_sub_##tag##_##o(void *p) { CAT(FIRST, fetch_sub_##tag##_##o)(p, N); } \
    static void b_sub_##tag##_##o(void *p) { CAT(SECOND, fetch_sub_##tag##_##o)(p, N); } \
    static void a_xor_##tag##_##o(void *p) { CAT(FIRST, xor_##tag##_##o)(p, N, 0x5a); } \
    static void b_xor_##tag##_##o(void *p) { CAT(SECOND, xor_##tag##_##o)(p, N, 0x33); } \
    static void a_bits_##tag##_##o(void *p) { lost_a_##tag##_##o = CAT(FIRST, or_and_##tag##_##o)(p, N, 1); } \
    static void b_bits_##tag##_##o(void *p) { lost_b_##tag##_##o = CAT(SECOND, or_and_##tag##_##o)(p, N, 2); } \
    static void test_arith_##tag##_##o(void) \
    { \
        A *v = &shared_##tag##_##o; \
        atomic_store(v, 0); \
        run_pair(a_add_##tag##_##o, b_add_##tag##_##o, (void *)v); \
        check("add " #tag " order " #o, atomic_load(v) == (T)(2 * N)); \
        atomic_store(v, 0); \
        run_pair(a_sub_##tag##_##o, b_sub_##tag##_##o, (void *)v); \
        check("sub " #tag " order " #o, atomic_load(v) == (T)(0 - 2 * N)); \
        atomic_store(v, 0x69); \
        run_pair(a_xor_##tag##_##o, b_xor_##tag##_##o, (void *)v); \
        check("xor " #tag " order " #o, atomic_load(v) == 0x69); \
        atomic_store(v, 0); \
        run_pair(a_bits_##tag##_##o, b_bits_##tag##_##o, (void *)v); \
        check("or/and " #tag " order " #o, \
              atomic_load(v) == 0 && lost_a_##tag##_##o == 0 && lost_b_##tag##_##o == 0); \
    }
#define ARITH_TESTS(tag, A, T) \
    ARITH_TEST(tag, A, T, 0) ARITH_TEST(tag, A, T, 1) ARITH_TEST(tag, A, T, 2) \
    ARITH_TEST(tag, A, T, 3) ARITH_TEST(tag, A, T, 4) ARITH_TEST(tag, A, T, 5)
WIDTHS(ARITH_TESTS)

/* Exchange locks: the plain count is exact only under mutual exclusion. */
#define LOCK_TEST(tag, A, T, o) \
    static A lock_##tag##_##o; \
    static long count_##tag##_##o; \
    static void a_lock_##tag##_##o(void *p) { (void)p; CAT(FIRST, exchange_lock_##tag##_##o)(&lock_##tag##_##o, &count_##tag##_##o, N); } \
    static void b_lock_##tag##_##o(void *p) { (void)p; CAT(SECOND, exchange_lock_##tag##_##o)(&lock_##tag##_##o, &count_##tag##_##o, N); } \
    static void test_lock_##tag##_##o(void) \
    { \
        atomic_store(&lock_##tag##_##o, 0); \
        count_##tag##_##o = 0; \
        run_pair(a_lock_##tag##_##o, b_lock_##tag##_##o, 0); \
        check("exchange lock " #tag " order " #o, \
              count_##tag##_##o == 2 * N && atomic_load(&lock_##tag##_##o) == 0); \
    }
#define LOCK_TESTS(tag, A, T) LOCK_TEST(tag, A, T, 2) LOCK_TEST(tag, A, T, 4) LOCK_TEST(tag, A, T, 5)
WIDTHS(LOCK_TESTS)

/* Compare-exchange loops: strong on one thread, weak on the other. */
#define CAS_TEST(tag, A, T, pair) \
    static A cas_##tag##_##pair; \
    static void a_cas_##tag##_##pair(void *p) { CAT(FIRST, cas_weak_##tag##_##pair)(p, N); } \
    static void b_cas_##tag##_##pair(void *p) { CAT(SECOND, cas_strong_##tag##_##pair)(p, N); } \
    static void c_cas_##tag##_##pair(void *p) { CAT(FIRST, cas_strong_##tag##_##pair)(p, N); } \
    static void d_cas_##tag##_##pair(void *p) { CAT(SECOND, cas_weak_##tag##_##pair)(p, N); } \
    static void test_cas_##tag##_##pair(void) \
    { \
        atomic_store(&cas_##tag##_##pair, 0); \
        run_pair(a_cas_##tag##_##pair, b_cas_##tag##_##pair, (void *)&cas_##tag##_##pair); \
        run_pair(c_cas_##tag##_##pair, d_cas_##tag##_##pair, (void *)&cas_##tag##_##pair); \
        check("compare-exchange " #tag " orders " #pair, \
              atomic_load(&cas_##tag##_##pair) == (T)(4 * N)); \
    }
#define CAS_TESTS(tag, A, T) \
    CAS_TEST(tag, A, T, 55) CAS_TEST(tag, A, T, 42) CAS_TEST(tag, A, T, 30) \
    CAS_TEST(tag, A, T, 21) CAS_TEST(tag, A, T, 00) CAS_TEST(tag, A, T, 05)
WIDTHS(CAS_TESTS)

/* Flag locks, pointer forms, a floating counter and fences. */
#define FLAG_TEST(pair) \
    static atomic_flag flag_##pair = ATOMIC_FLAG_INIT; \
    static long flag_count_##pair; \
    static void a_flag_##pair(void *p) { (void)p; CAT(FIRST, flag_lock_##pair)(&flag_##pair, &flag_count_##pair, N); } \
    static void b_flag_##pair(void *p) { (void)p; CAT(SECOND, flag_lock_##pair)(&flag_##pair, &flag_count_##pair, N); } \
    static void test_flag_##pair(void) \
    { \
        atomic_flag_clear(&flag_##pair); \
        flag_count_##pair = 0; \
        run_pair(a_flag_##pair, b_flag_##pair, 0); \
        check("flag lock orders " #pair, \
              flag_count_##pair == 2 * N && !atomic_flag_test_and_set(&flag_##pair)); \
    }
FLAG_TEST(23) FLAG_TEST(43) FLAG_TEST(55)

static int cells[4];
#define POINTER_TEST(o) \
    static _Atomic(int *) cursor_##o; \
    static void a_pointer_##o(void *p) { CAT(FIRST, pointer_##o)(p, N); } \
    static void b_pointer_##o(void *p) { CAT(SECOND, pointer_##o)(p, N); } \
    static void test_pointer_##o(void) \
    { \
        atomic_store(&cursor_##o, cells); \
        run_pair(a_pointer_##o, b_pointer_##o, (void *)&cursor_##o); \
        check("pointer elements order " #o, \
              (char *)atomic_load(&cursor_##o) - (char *)cells == 2L * N * 3 * sizeof(int)); \
    }
POINTER_TEST(0) POINTER_TEST(2) POINTER_TEST(3) POINTER_TEST(5)

static _Atomic(double) total;
static void a_double(void *p) { CAT(FIRST, double_add)(p, N); }
static void b_double(void *p) { CAT(SECOND, double_add)(p, N); }
static void test_double(void)
{
    atomic_store(&total, 0.0);
    run_pair(a_double, b_double, (void *)&total);
    check("floating compare-exchange", atomic_load(&total) == 2.0 * N);
}

static atomic_int data, flag, x, y;
static int rx, ry, received;
static void a_publish(void *p) { (void)p; CAT(FIRST, publish)(&data, &flag); }
static void b_receive(void *p) { (void)p; received = CAT(SECOND, receive)(&data, &flag); }
static void c_publish(void *p) { (void)p; CAT(SECOND, publish)(&data, &flag); }
static void d_receive(void *p) { (void)p; received = CAT(FIRST, receive)(&data, &flag); }
static void a_buffer(void *p) { (void)p; rx = CAT(FIRST, store_buffer)(&x, &y); }
static void b_buffer(void *p) { (void)p; ry = CAT(SECOND, store_buffer)(&y, &x); }

static void test_fences(void)
{
    int late = 0, forbidden = 0;
    for (int round = 0; round < ROUNDS; round++) {
        atomic_store(&data, 0);
        atomic_store(&flag, 0);
        run_pair(round % 2 ? a_publish : b_receive, round % 2 ? b_receive : a_publish, 0);
        late += received != 42;
        atomic_store(&data, 0);
        atomic_store(&flag, 0);
        run_pair(round % 2 ? c_publish : d_receive, round % 2 ? d_receive : c_publish, 0);
        late += received != 42;
        atomic_store(&x, 0);
        atomic_store(&y, 0);
        run_pair(a_buffer, b_buffer, 0);
        forbidden += rx == 0 && ry == 0;
    }
    check("fence message passing", late == 0);
    check("fence store buffering", forbidden == 0);
}

int main(void)
{
#define RUN_WIDTH(tag, A, T) \
    test_arith_##tag##_0(); test_arith_##tag##_1(); test_arith_##tag##_2(); \
    test_arith_##tag##_3(); test_arith_##tag##_4(); test_arith_##tag##_5(); \
    test_lock_##tag##_2(); test_lock_##tag##_4(); test_lock_##tag##_5(); \
    test_cas_##tag##_55(); test_cas_##tag##_42(); test_cas_##tag##_30(); \
    test_cas_##tag##_21(); test_cas_##tag##_00(); test_cas_##tag##_05();
    WIDTHS(RUN_WIDTH)
    test_flag_23(); test_flag_43(); test_flag_55();
    test_pointer_0(); test_pointer_2(); test_pointer_3(); test_pointer_5();
    test_double();
    test_fences();
    printf("failures=%d tests=%d\n", failures, tests);
    return failures != 0;
}
