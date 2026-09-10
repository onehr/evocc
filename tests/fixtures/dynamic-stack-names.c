/* Binding probe. Exit 0 means stack storage; exit 11 means the written body. */
static unsigned char storage[32];
static int calls;

#if CASE == 1
extern void *alloca(unsigned long);
#elif CASE == 2
void *alloca(unsigned long n) { ++calls; return storage; }
#elif CASE == 3
static void *alloca(unsigned long n) { ++calls; return storage; }
#elif CASE == 4
void *replacement(unsigned long n) { ++calls; return storage; }
extern void *alloca(unsigned long) __asm__("replacement");
#endif

int main(void) {
    unsigned char *p = alloca(17);
    p[0] = 17;
    p[16] = 29;
    if (p[0] + p[16] != 46) return 99;
    return calls * 10 + (p == storage);
}
