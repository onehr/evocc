/* Query-only knowledge must not turn object reads into source ICEs. */
#define IS_ICE(x) (sizeof(int) == sizeof(*(8 ? (void *)((long)(x) * 0L) : (int *)8)))
typedef unsigned long size_t;
#define SIZE_MAX (~(size_t)0)
#define __member_size(p) __builtin_dynamic_object_size(p, 1)
/* The frozen Linux fortify-string.h macro, with its query guards intact. */
#define __compiletime_strlen(p)                                      \
({                                                                  \
    char *__p = (char *)(p);                                         \
    size_t __ret = SIZE_MAX;                                         \
    const size_t __p_size = __member_size(p);                         \
    if (__p_size != SIZE_MAX && __builtin_constant_p(*__p)) {         \
        size_t __p_len = __p_size - 1;                               \
        if (__builtin_constant_p(__p[__p_len]) && __p[__p_len] == '\0') \
            __ret = __builtin_strlen(__p);                           \
    }                                                               \
    __ret;                                                          \
})
size_t strlen(const char *s) {
    size_t n = 0;
    while (s[n]) ++n;
    return n;
}
int main(void) {
    const char *p = "88888888";
    const char *bytes = "\xff\0z";
    const signed char *signed_bytes = (const signed char *)bytes;
    int unknown;
    if (__builtin_dynamic_object_size(p, 1) != 9) return 1;
    if (!__builtin_constant_p("88888888"[0])) return 2;
    if (!__builtin_constant_p(*p)) return 3;
    if (!__builtin_constant_p(p[8])) return 4;
    if (!__builtin_constant_p(bytes[1]) || !__builtin_constant_p(bytes[2])) return 5;
    if (!__builtin_constant_p(bytes[0] == 255 ? 7 : unknown)) return 6;
    if (!__builtin_constant_p(signed_bytes[0] == -1 ? 7 : unknown)) return 7;
    if (IS_ICE("x"[0]) || IS_ICE(p[0]) || IS_ICE(__builtin_strlen(p))) return 8;
    if (__compiletime_strlen("88888888") != 8) return 9;
    return __builtin_strlen(p) != 8;
}
