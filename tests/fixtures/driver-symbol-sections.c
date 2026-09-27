/* One source for section placement, linker GC, TLS and lifecycle regressions. */
extern void abort(void);

int initialized = 7;
int zeroed;
const int readonly = 11;
const int readonly_zero;
int *const relocation = &initialized;
__thread int tls_data = 13;
__thread int tls_zero;
int explicit_data __attribute__((section(".data.selected"))) = 17;
int explicit_zero __attribute__((section(".bss.selected")));
__thread int explicit_tls __attribute__((section(".tdata.selected"))) = 19;

int dead_data = 101;
int dead_zero;
const int dead_readonly = 103;
__thread int dead_tls = 107;
__thread int dead_tls_zero;
int dead_function(void) { return dead_data + dead_zero + dead_readonly; }

int live_function(void) { return initialized; }
extern int live_alias(void) __attribute__((alias("live_function")));
int selected_function(void) __attribute__((section(".text.selected")));
int selected_function(void) { return explicit_data; }

int dispatch(int selector) {
    static void *const labels[] = { &&a, &&b };
    goto *labels[selector];
a:
    return 23;
b:
    return 29;
}

static volatile int lifecycle;
static void init(void) __attribute__((constructor(201)));
static void fini(void) __attribute__((destructor(201)));
static void init(void) { lifecycle = 31; }
static void fini(void) { if (lifecycle != 37) abort(); }

int main(void) {
    int failed = live_alias() != 7 || live_alias != live_function ||
        selected_function() != 17 || zeroed != 0 || readonly != 11 ||
        readonly_zero != 0 || relocation != &initialized ||
        tls_data != 13 || tls_zero != 0 || explicit_tls != 19 ||
        explicit_zero != 0 || dispatch(0) != 23 || dispatch(1) != 29 ||
        lifecycle != 31;
    lifecycle = 37;
    return failed;
}
