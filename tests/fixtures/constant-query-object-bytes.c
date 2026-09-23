#define IS_ICE(x) (sizeof(int) == sizeof(*(8 ? (void *)((long)(x) * 0L) : (int *)8)))
typedef unsigned long size_t;
#define SIZE_MAX (~(size_t)0)
#define compiletime_strlen(p) ({ \
    char *q = (char *)(p); \
    size_t result = SIZE_MAX; \
    const size_t size = __builtin_dynamic_object_size(p, 1); \
    if (size != SIZE_MAX && __builtin_constant_p(*q)) { \
        size_t end = size - 1; \
        if (__builtin_constant_p(q[end]) && q[end] == 0) \
            result = __builtin_strlen(q); \
    } \
    result; \
})
static const char array[] = "ten bytes!";
static const char *pointer = "eleven!!!!!";
static const char *const fixed_pointer = "twelve!!!!!!";
static char mutable_array[] = "unknown";
static volatile char initial = 'u';
size_t strlen(const char *p) { size_t n = 0; while (p[n]) ++n; return n; }
int main(void)
{
    char local[80] = "Test!";
    if (compiletime_strlen(local) != 5) return 1;
    if (compiletime_strlen(array) != 10) return 2;
    if (compiletime_strlen(pointer) != 11) return 3;
    if (compiletime_strlen(fixed_pointer) != 12) return 4;
    mutable_array[0] = initial;
    if (compiletime_strlen(mutable_array) != SIZE_MAX) return 5;
    return IS_ICE(local[0]) || IS_ICE(array[0]);
}
