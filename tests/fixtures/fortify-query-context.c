typedef unsigned long size_t;
static char storage[256];
static char *volatile backing = storage;
static int calls;
struct context { const char *file; int line; char *pointer; };

__attribute__((noinline, alloc_size(2)))
static void *allocate(void *device, size_t size, unsigned flags)
{
    ++calls;
    return backing;
}

static inline _Bool checked(_Bool overflow)
{
    return __builtin_expect(!!overflow, 0);
}

static inline void *array(void *device, size_t count, size_t size, unsigned flags)
{
    size_t bytes;
    if (__builtin_expect(!!checked(__builtin_mul_overflow(count, size, &bytes)), 0))
        return (void *)0;
    return allocate(device, bytes, flags);
}

__attribute__((noinline)) static int local_bytes(struct context *context)
{
    char local[80] = "Test!";
    *(const char *volatile *)&context->file = "here";
    *(volatile int *)&context->line = 7;
    if (!__builtin_constant_p(local[0]) || !__builtin_constant_p(local[79]))
        return 1;
    context->pointer = local;
    *context->pointer = 'z';
    return local[0] != 'z';
}

int main(void)
{
    void *a = array(0, 1, 11, 3);
    void *b = array(0, 128, 1, 3);
    void *c = array(0, 1, 0, 3);
    if (__builtin_object_size(a, 1) != 11 ||
        __builtin_object_size(b, 1) != 128 ||
        __builtin_object_size(c, 1) != 0 || calls != 3)
        return 1;
    if (array(0, ~(size_t)0, 2, 3) != 0 || calls != 3)
        return 2;
    struct context context = {0};
    return local_bytes(&context) || context.line != 7;
}
