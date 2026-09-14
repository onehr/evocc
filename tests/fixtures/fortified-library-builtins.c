/* Ordinary definitions let both interpreters exercise builtin call binding. */
typedef unsigned long size_t;

__attribute__((noinline)) char *strcat(char *dst, const char *src)
{
    char *result = dst;
    while (*dst)
        ++dst;
    while ((*dst++ = *src++)) {}
    return result;
}

__attribute__((noinline)) char *strncat(char *dst, const char *src, size_t count)
{
    char *result = dst;
    while (*dst)
        ++dst;
    while (count && *src) {
        *dst++ = *src++;
        --count;
    }
    *dst = 0;
    return result;
}

__attribute__((noinline)) void *memchr(const void *ptr, int value, size_t count)
{
    const unsigned char *p = ptr;
    for (size_t i = 0; i < count; ++i)
        if (p[i] == (unsigned char)value)
            return (void *)(p + i);
    return (void *)0;
}

static int destinations, sources, counts;
static char *destination(char *p) { ++destinations; return p; }
static const char *source(const char *p) { ++sources; return p; }
static size_t length(size_t n) { ++counts; return n; }

int main(void)
{
    char text[16] = "ab";
    if (__builtin_strcat(destination(text), source("cd")) != text)
        return 1;
    if (__builtin_strncat(destination(text), source("efgh"), length(2)) != text)
        return 2;
    if (text[0] != 'a' || text[3] != 'd' || text[4] != 'e'
        || text[5] != 'f' || text[6] != 0)
        return 3;
    if (destinations != 2 || sources != 2 || counts != 1)
        return 4;
    if (__builtin_memchr(text, 'c' + 256, 6) != text + 2
        || __builtin_memchr(text, 'g', 6) != (void *)0
        || __builtin_memchr(text, 'a', 0) != (void *)0)
        return 5;
    if (__builtin_strncat(text, "XYZ", 0) != text || text[6] != 0)
        return 6;
    return 0;
}
