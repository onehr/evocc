typedef unsigned long size_t;
static char storage[128];
static char *volatile storage_pointer = storage;
static int calls;
static volatile size_t supplied_size = 25;
__attribute__((noinline, alloc_size(1)))
static void *allocate(size_t size) { ++calls; return storage_pointer; }

static void * __attribute__((alloc_size(2))) device_allocate(void *device, size_t size);
__attribute__((noinline))
static void *device_allocate(void *device, size_t size) { ++calls; return storage_pointer; }

#define hooks(expression) ({ \
    void *result; \
    if (0) result = (expression); else result = (expression); \
    result; \
})

int main(void) {
    size_t size = supplied_size;
    void *first = hooks(allocate(size++));
    void *second = hooks(allocate(37));
    if (__builtin_dynamic_object_size(first, 1) != 25) return 1;
    if (__builtin_object_size(first, 1) != (size_t)-1) return 2;
    if (__builtin_dynamic_object_size(second, 1) != 37) return 3;
    if (__builtin_object_size(second, 1) != 37) return 4;
    void *third = device_allocate(0, size++);
    if (__builtin_dynamic_object_size(third, 1) != 26) return 5;
    size_t fixed;
    fixed = 11;
    if (!__builtin_constant_p(1)) fixed += supplied_size;
    void *fourth = allocate(fixed);
    fixed = 7;
    if (__builtin_object_size(fourth, 1) != 11) return 6;
    return size != 27 || calls != 4 || fixed != 7;
}
