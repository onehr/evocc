typedef unsigned long size_t;
static char storage[128];
static char *volatile backing = storage;
static int calls;

__attribute__((noinline, alloc_size(1)))
static void *allocate(size_t bytes)
{
    ++calls;
    return backing;
}

static inline void *array(size_t count, size_t size)
{
    size_t bytes;
    if (__builtin_mul_overflow(count, size, &bytes))
        return (void *)0;
    return allocate(bytes);
}

static inline char *selected(int choice, size_t size)
{
    if (choice)
        return (char *)allocate(size++) + 2;
    return allocate(7);
}

static inline char *nested(int choice, size_t size)
{
    return selected(choice, size);
}

int main(void)
{
    volatile size_t input = 17;
    size_t size = input;
    void *p = array(1, size++);
    if (calls != 1 || size != 18)
        return 1;
    if (__builtin_dynamic_object_size(p, 1) != 17)
        return 2;
    if (array(~(size_t)0, 2) != (void *)0 || calls != 1)
        return 3;
    volatile int choice = 1;
    char *a = nested(choice++, size++);
    choice = 0;
    char *b = nested(choice++, size++);
    choice = 99;
    size = 88;
    if (calls != 3 || __builtin_dynamic_object_size(a, 0) != 16 ||
        __builtin_dynamic_object_size(a, 1) != 16 ||
        __builtin_dynamic_object_size(b, 0) != 7 ||
        __builtin_dynamic_object_size(b, 1) != 7)
        return 4;
    return choice != 99 || size != 88;
}
