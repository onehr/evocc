/* K07 probe: ordinary-call effects next to the folded forms.
 * CASE=0: a user-defined strlen counts its calls. The exit value packs which
 * operands were folded (bit set) and how many calls ran (bits 6..7).
 * CASE=1: the plain strlen spelling with only a declaration; the exit value
 * is the classification mask (no call executes inside sizeof). */
typedef unsigned long size_t;
#define is_constexpr(x) \
    (sizeof(int) == sizeof(*(8 ? (void *)((long)(x) * 0L) : (int *)8)))

static const char array_of_10[] = "this is 10";
static const char *const unchanging_12 = "this is 12!!";
static const char *ptr_of_11 = "this is 11!";

#if CASE == 0
static int calls;
size_t strlen(const char *s) { (void)s; calls++; return 1000; }

int main(void)
{
    size_t a = __builtin_strlen(array_of_10);
    size_t b = __builtin_strlen(unchanging_12);
    size_t c = __builtin_strlen(ptr_of_11);
    return (a == 10) | ((a == 1000) << 1) | ((b == 12) << 2)
        | ((b == 1000) << 3) | ((c == 11) << 4) | ((c == 1000) << 5)
        | (calls << 6);
}
#elif CASE == 1
size_t strlen(const char *);

int main(void)
{
    return is_constexpr(strlen("88888888"))
        | (is_constexpr(strlen(array_of_10)) << 1)
        | (is_constexpr(strlen(unchanging_12)) << 2);
}
#endif
