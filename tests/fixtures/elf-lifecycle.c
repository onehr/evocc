extern int puts(const char *);
extern void exit(int);
extern void _Exit(int);
extern void quick_exit(int);
static int state;
static void first(void) __attribute__((constructor(201)));
static void second(void) __attribute__((constructor(202)));
static void last(void) __attribute__((destructor(201)));
static void before_last(void) __attribute__((destructor(202)));
static void first(void) { state = 1; puts("init 201"); }
static void second(void) { if(state!=1)__builtin_trap(); state=2; puts("init 202"); }
static void before_last(void) { if(state!=3)__builtin_trap();state=4;puts("fini 202"); }
static void last(void) { if(state!=4)__builtin_trap();puts("fini 201"); }
int main(void) {
    if(state!=2)return 1;
    state=3;
    puts("main");
#if MODE == 1
    exit(0);
#elif MODE == 2
    _Exit(0);
#elif MODE == 3
    quick_exit(0);
#endif
    return 0;
}
