typedef unsigned long size_t;
static char storage[128];
static char *volatile backing = storage;
static int allocations, effects;

__attribute__((noinline, alloc_size(1))) void *allocate(size_t n)
{
    ++allocations;
    return backing;
}

static inline void *wrapper(size_t n)
{
    ++effects;
    void *result = allocate(n);
    ++effects;
    return result;
}

static inline char *offset_wrapper(size_t n)
{
    char *result = wrapper(n);
    return result + 3;
}

int main(void)
{
    void *first = wrapper(11);
    if (__builtin_object_size(first, 1) != 11
        || __builtin_dynamic_object_size(first, 1) != 11)
        return 1;
    volatile size_t input = 25;
    size_t n = input;
    char *second = offset_wrapper(n++);
    return __builtin_dynamic_object_size(second, 1) != 22
        || n != 26 || allocations != 2 || effects != 4;
}
