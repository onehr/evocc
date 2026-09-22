typedef unsigned long size_t;
static char storage[128];
static char *volatile backing = storage;
static volatile size_t zero_size;
static int calls, observations;
static int marker;

__attribute__((noinline, alloc_size(2)))
static void *duplicate(const void *source, size_t length)
{
    ++calls;
    return backing;
}

__attribute__((noinline)) static void observe(void *pointer)
{
    if (pointer)
        ++observations;
}

#define CHECK(expected_size) do {                                    \
    size_t length;                                                  \
    length = 11;                                                    \
    if (!__builtin_constant_p(expected_size))                        \
        length += zero_size;                                        \
    do {                                                            \
        size_t expected = length;                                   \
        void *pointer = duplicate("hello there", length);            \
        *(volatile int *)location = 1;                              \
        observe(pointer);                                           \
        if (__builtin_object_size(pointer, 1) != expected) return 1; \
        if (__builtin_dynamic_object_size(pointer, 1) != expected)   \
            return 2;                                               \
    } while (0);                                                    \
} while (0)

__attribute__((noinline)) static int test(int *location)
{
    CHECK(0);
    CHECK(128);
    return calls != 2 || observations != 2;
}

int main(void) { return test(&marker) || marker != 1; }
