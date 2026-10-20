/* L11b: run two builds of atomic-gnu-crossed-worker.c in two threads on the
 * same objects. Build with -DFIRST=name -DSECOND=name. Each test starts both
 * threads together; thread one calls FIRST, thread two calls SECOND. The
 * exit status is 0 when every test passes. A pass shows no lost update and
 * no forbidden outcome in these runs; it is not a memory-model proof. */
#include <pthread.h>
#include <stdio.h>

#define CAT2(a, b) a##_##b
#define CAT(a, b) CAT2(a, b)
#define N 20000
#define ROUNDS 2000

#define WIDTHS(X) \
    X(u8, unsigned char) X(u16, unsigned short) X(u32, unsigned) X(u64, unsigned long)

static int ready, failures, tests;
struct job { void (*run)(void *); void *arg; };

static void *start(void *raw)
{
    struct job *job = raw;
    __atomic_add_fetch(&ready, 1, 5);
    while (__atomic_load_n(&ready, 5) < 2) { }
    job->run(job->arg);
    return 0;
}

static void run_pair(void (*first)(void *), void (*second)(void *), void *arg)
{
    pthread_t a, b;
    struct job one = { first, arg }, two = { second, arg };
    __atomic_store_n(&ready, 0, 5);
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
#define ARITH_DECL(side, tag, T, o) \
    void CAT(side, fetch_add_##tag##_##o)(T *, int); \
    void CAT(side, add_fetch_##tag##_##o)(T *, int); \
    void CAT(side, fetch_sub_##tag##_##o)(T *, int); \
    void CAT(side, sub_fetch_##tag##_##o)(T *, int); \
    void CAT(side, xor_##tag##_##o)(T *, int, T); \
    void CAT(side, nand_##tag##_##o)(T *, int); \
    int CAT(side, or_and_##tag##_##o)(T *, int, T);
#define LOCK_DECL(side, tag, T, o) void CAT(side, exchange_lock_##tag##_##o)(T *, long *, int);
#define CAS_DECL(side, tag, T, pair) \
    void CAT(side, cas_weak_##tag##_##pair)(T *, int); \
    void CAT(side, cas_strong_##tag##_##pair)(T *, int);
#define SIDE_DECL(side) \
    void CAT(side, flag_lock_23)(_Bool *, long *, int); \
    void CAT(side, flag_lock_43)(_Bool *, long *, int); \
    void CAT(side, flag_lock_55)(_Bool *, long *, int); \
    void CAT(side, pointer_0)(int **, int); \
    void CAT(side, pointer_2)(int **, int); \
    void CAT(side, pointer_3)(int **, int); \
    void CAT(side, pointer_5)(int **, int); \
    void CAT(side, publish)(int *, int *); \
    int CAT(side, receive)(int *, int *); \
    int CAT(side, store_buffer)(int *, int *);
#define WIDTH_DECL(tag, T) \
    ARITH_DECL(FIRST, tag, T, 0) ARITH_DECL(FIRST, tag, T, 1) ARITH_DECL(FIRST, tag, T, 2) \
    ARITH_DECL(FIRST, tag, T, 3) ARITH_DECL(FIRST, tag, T, 4) ARITH_DECL(FIRST, tag, T, 5) \
    ARITH_DECL(SECOND, tag, T, 0) ARITH_DECL(SECOND, tag, T, 1) ARITH_DECL(SECOND, tag, T, 2) \
    ARITH_DECL(SECOND, tag, T, 3) ARITH_DECL(SECOND, tag, T, 4) ARITH_DECL(SECOND, tag, T, 5) \
    LOCK_DECL(FIRST, tag, T, 2) LOCK_DECL(FIRST, tag, T, 4) LOCK_DECL(FIRST, tag, T, 5) \
    LOCK_DECL(SECOND, tag, T, 2) LOCK_DECL(SECOND, tag, T, 4) LOCK_DECL(SECOND, tag, T, 5) \
    CAS_DECL(FIRST, tag, T, 55) CAS_DECL(FIRST, tag, T, 42) CAS_DECL(FIRST, tag, T, 30) \
    CAS_DECL(FIRST, tag, T, 21) CAS_DECL(FIRST, tag, T, 11) CAS_DECL(FIRST, tag, T, 00) \
    CAS_DECL(SECOND, tag, T, 55) CAS_DECL(SECOND, tag, T, 42) CAS_DECL(SECOND, tag, T, 30) \
    CAS_DECL(SECOND, tag, T, 21) CAS_DECL(SECOND, tag, T, 11) CAS_DECL(SECOND, tag, T, 00)
WIDTHS(WIDTH_DECL)
SIDE_DECL(FIRST)
SIDE_DECL(SECOND)

/* Arithmetic: counters, xor and nand toggles, and owned bits. */
#define ARITH_TEST(tag, T, o) \
    struct bits_##tag##_##o { T *p; T mask; int lost; }; \
    static void a_add_##tag##_##o(void *p) { CAT(FIRST, fetch_add_##tag##_##o)(p, N); } \
    static void b_add_##tag##_##o(void *p) { CAT(SECOND, add_fetch_##tag##_##o)(p, N); } \
    static void a_sub_##tag##_##o(void *p) { CAT(FIRST, fetch_sub_##tag##_##o)(p, N); } \
    static void b_sub_##tag##_##o(void *p) { CAT(SECOND, sub_fetch_##tag##_##o)(p, N); } \
    static void a_nand_##tag##_##o(void *p) { CAT(FIRST, nand_##tag##_##o)(p, N); } \
    static void b_nand_##tag##_##o(void *p) { CAT(SECOND, nand_##tag##_##o)(p, N); } \
    static T shared_##tag##_##o; \
    static void a_xor_##tag##_##o(void *p) { CAT(FIRST, xor_##tag##_##o)(p, N, 0x5a); } \
    static void b_xor_##tag##_##o(void *p) { CAT(SECOND, xor_##tag##_##o)(p, N, 0x33); } \
    static int lost_a_##tag##_##o, lost_b_##tag##_##o; \
    static void a_bits_##tag##_##o(void *p) { lost_a_##tag##_##o = CAT(FIRST, or_and_##tag##_##o)(p, N, 1); } \
    static void b_bits_##tag##_##o(void *p) { lost_b_##tag##_##o = CAT(SECOND, or_and_##tag##_##o)(p, N, 2); } \
    static void test_arith_##tag##_##o(void) \
    { \
        T *v = &shared_##tag##_##o; \
        *v = 0; \
        run_pair(a_add_##tag##_##o, b_add_##tag##_##o, v); \
        check("add " #tag " order " #o, *v == (T)(2 * N)); \
        *v = 0; \
        run_pair(a_sub_##tag##_##o, b_sub_##tag##_##o, v); \
        check("sub " #tag " order " #o, *v == (T)(0 - 2 * N)); \
        *v = 0x69; \
        run_pair(a_xor_##tag##_##o, b_xor_##tag##_##o, v); \
        check("xor " #tag " order " #o, *v == 0x69); \
        *v = 0x96; \
        run_pair(a_nand_##tag##_##o, b_nand_##tag##_##o, v); \
        check("nand " #tag " order " #o, *v == 0x96); \
        *v = 0; \
        run_pair(a_bits_##tag##_##o, b_bits_##tag##_##o, v); \
        check("or/and " #tag " order " #o, \
              *v == 0 && lost_a_##tag##_##o == 0 && lost_b_##tag##_##o == 0); \
    }
#define ARITH_TESTS(tag, T) \
    ARITH_TEST(tag, T, 0) ARITH_TEST(tag, T, 1) ARITH_TEST(tag, T, 2) \
    ARITH_TEST(tag, T, 3) ARITH_TEST(tag, T, 4) ARITH_TEST(tag, T, 5)
WIDTHS(ARITH_TESTS)

/* Exchange locks: the plain count is exact only under mutual exclusion. */
#define LOCK_TEST(tag, T, o) \
    static T lock_##tag##_##o; \
    static long count_##tag##_##o; \
    static void a_lock_##tag##_##o(void *p) { (void)p; CAT(FIRST, exchange_lock_##tag##_##o)(&lock_##tag##_##o, &count_##tag##_##o, N); } \
    static void b_lock_##tag##_##o(void *p) { (void)p; CAT(SECOND, exchange_lock_##tag##_##o)(&lock_##tag##_##o, &count_##tag##_##o, N); } \
    static void test_lock_##tag##_##o(void) \
    { \
        lock_##tag##_##o = 0; \
        count_##tag##_##o = 0; \
        run_pair(a_lock_##tag##_##o, b_lock_##tag##_##o, 0); \
        check("exchange lock " #tag " order " #o, count_##tag##_##o == 2 * N && lock_##tag##_##o == 0); \
    }
#define LOCK_TESTS(tag, T) LOCK_TEST(tag, T, 2) LOCK_TEST(tag, T, 4) LOCK_TEST(tag, T, 5)
WIDTHS(LOCK_TESTS)

/* Compare-exchange loops: strong on one thread, weak on the other. */
#define CAS_TEST(tag, T, pair) \
    static T cas_##tag##_##pair; \
    static void a_cas_##tag##_##pair(void *p) { CAT(FIRST, cas_weak_##tag##_##pair)(p, N); } \
    static void b_cas_##tag##_##pair(void *p) { CAT(SECOND, cas_strong_##tag##_##pair)(p, N); } \
    static void c_cas_##tag##_##pair(void *p) { CAT(FIRST, cas_strong_##tag##_##pair)(p, N); } \
    static void d_cas_##tag##_##pair(void *p) { CAT(SECOND, cas_weak_##tag##_##pair)(p, N); } \
    static void test_cas_##tag##_##pair(void) \
    { \
        cas_##tag##_##pair = 0; \
        run_pair(a_cas_##tag##_##pair, b_cas_##tag##_##pair, &cas_##tag##_##pair); \
        run_pair(c_cas_##tag##_##pair, d_cas_##tag##_##pair, &cas_##tag##_##pair); \
        check("compare-exchange " #tag " orders " #pair, cas_##tag##_##pair == (T)(4 * N)); \
    }
#define CAS_TESTS(tag, T) \
    CAS_TEST(tag, T, 55) CAS_TEST(tag, T, 42) CAS_TEST(tag, T, 30) \
    CAS_TEST(tag, T, 21) CAS_TEST(tag, T, 11) CAS_TEST(tag, T, 00)
WIDTHS(CAS_TESTS)

/* Test-and-set locks, pointer forms and fences. */
#define FLAG_TEST(pair) \
    static _Bool flag_##pair; \
    static long flag_count_##pair; \
    static void a_flag_##pair(void *p) { (void)p; CAT(FIRST, flag_lock_##pair)(&flag_##pair, &flag_count_##pair, N); } \
    static void b_flag_##pair(void *p) { (void)p; CAT(SECOND, flag_lock_##pair)(&flag_##pair, &flag_count_##pair, N); } \
    static void test_flag_##pair(void) \
    { \
        flag_##pair = 0; \
        flag_count_##pair = 0; \
        run_pair(a_flag_##pair, b_flag_##pair, 0); \
        check("test-and-set lock orders " #pair, flag_count_##pair == 2 * N && !flag_##pair); \
    }
FLAG_TEST(23) FLAG_TEST(43) FLAG_TEST(55)

static int cells[4];
#define POINTER_TEST(o) \
    static int *cursor_##o; \
    static void a_pointer_##o(void *p) { CAT(FIRST, pointer_##o)(p, N); } \
    static void b_pointer_##o(void *p) { CAT(SECOND, pointer_##o)(p, N); } \
    static void test_pointer_##o(void) \
    { \
        cursor_##o = cells; \
        run_pair(a_pointer_##o, b_pointer_##o, &cursor_##o); \
        check("pointer bytes order " #o, \
              (char *)cursor_##o - (char *)cells == 2L * N * 3); \
    }
POINTER_TEST(0) POINTER_TEST(2) POINTER_TEST(3) POINTER_TEST(5)

static int data, flag, x, y, rx, ry, received;
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
        data = flag = 0;
        run_pair(round % 2 ? a_publish : b_receive, round % 2 ? b_receive : a_publish, 0);
        late += received != 42;
        data = flag = 0;
        run_pair(round % 2 ? c_publish : d_receive, round % 2 ? d_receive : c_publish, 0);
        late += received != 42;
        x = y = 0;
        run_pair(a_buffer, b_buffer, 0);
        forbidden += rx == 0 && ry == 0;
    }
    check("fence message passing", late == 0);
    check("fence store buffering", forbidden == 0);
}

int main(void)
{
#define RUN_ARITH(tag, T) \
    test_arith_##tag##_0(); test_arith_##tag##_1(); test_arith_##tag##_2(); \
    test_arith_##tag##_3(); test_arith_##tag##_4(); test_arith_##tag##_5(); \
    test_lock_##tag##_2(); test_lock_##tag##_4(); test_lock_##tag##_5(); \
    test_cas_##tag##_55(); test_cas_##tag##_42(); test_cas_##tag##_30(); \
    test_cas_##tag##_21(); test_cas_##tag##_11(); test_cas_##tag##_00();
    WIDTHS(RUN_ARITH)
    test_flag_23(); test_flag_43(); test_flag_55();
    test_pointer_0(); test_pointer_2(); test_pointer_3(); test_pointer_5();
    test_fences();
    printf("failures=%d tests=%d\n", failures, tests);
    return failures != 0;
}
