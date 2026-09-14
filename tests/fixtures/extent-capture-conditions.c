typedef unsigned long size_t;
static char storage[512];
static char *volatile base = storage;
static int calls;

__attribute__((noinline))
static char *choose(int present) {
    ++calls;
    return present ? base : (char *)0;
}

__attribute__((noinline, alloc_size(1)))
static void *allocate(size_t size) { return base + (size & 1); }

__attribute__((noinline, alloc_size(1)))
static void *allocate_byte(unsigned char size) { return base + (size & 1); }

__attribute__((noinline))
static int selected(int present) {
    char a[4], b[9];
    char *p = choose(present) ? a : b;
    return __builtin_dynamic_object_size(p, 0) != (present ? 4ul : 9ul);
}

int main(void) {
    size_t size = 13;
    char *p = allocate(*&size);
    size = 1;
    if (__builtin_dynamic_object_size(p, 0) != 13) return 1;
    size = 257;
    char *q = allocate_byte(size);
    if (__builtin_dynamic_object_size(q, 0) != 1) return 2;
    if (selected(0) || selected(1) || calls != 2) return 3;
    return 0;
}
