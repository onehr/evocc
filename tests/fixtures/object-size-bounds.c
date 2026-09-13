extern void invalid(void);
struct volume { char head[7]; char name[16]; char tail[5]; };
struct super { struct volume *disk; };

__attribute__((noinline)) int member(struct super *s)
{
    const unsigned long n = __builtin_object_size(s->disk->name, 1);
    if (!__builtin_constant_p(n) || n == 0 || n == ~0ul)
        invalid();
    return n != 16 || __builtin_object_size(s->disk->name + 3, 3) != 13;
}

int main(void)
{
    struct volume v;
    struct super s = { &v };
    char a[16];
    if (__builtin_object_size("abc", 1) != 4 ||
        __builtin_object_size(v.name, 0) != 21 ||
        __builtin_object_size(v.name, 1) != 16 ||
        __builtin_object_size(v.name, 2) != 21 ||
        __builtin_object_size(v.name, 3) != 16 ||
        __builtin_object_size(&a[3], 1) != 13 ||
        __builtin_object_size(a + 16, 1) != 0 ||
        __builtin_object_size((void *)a, 0) != 16)
        invalid();
    return member(&s);
}
