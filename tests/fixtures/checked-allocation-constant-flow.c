typedef unsigned long size_t;
static char storage[256];
static char *volatile backing = storage;
static int calls;

__attribute__((noinline, alloc_size(1)))
static void *allocate(size_t bytes)
{
    ++calls;
    return backing;
}

static inline _Bool checked(_Bool overflow)
{
    return __builtin_expect(overflow, 0);
}

static inline void *array(size_t count, size_t size)
{
    size_t bytes;
    if (checked(__builtin_mul_overflow(count, size, &bytes)))
        return (void *)0;
    return allocate(bytes);
}

int main(void)
{
    void *a = array(1, 11);
    void *b = array(128, 1);
    void *c = array(1, 0);
    if (__builtin_object_size(a, 1) != 11 ||
        __builtin_object_size(b, 1) != 128 ||
        __builtin_object_size(c, 1) != 0 || calls != 3)
        return 1;
    if (array(~(size_t)0, 2) != 0 || calls != 3)
        return 2;
    size_t wrapped;
    if (!checked(__builtin_add_overflow(~(size_t)0, 1, &wrapped)))
        return 3;
    if (wrapped != 0)
        return 4;
    volatile size_t observed;
    if (checked(__builtin_mul_overflow(3, 7, &observed)))
        return 5;
    if (observed != 21)
        return 6;
    size_t outputs[2] = {0, 0};
    int index = 0;
    if (checked(__builtin_mul_overflow(3, 7, &outputs[index++])))
        return 7;
    return index != 1 || outputs[0] != 21 || outputs[1] != 0;
}
