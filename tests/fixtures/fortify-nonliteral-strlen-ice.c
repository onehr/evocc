/* Original FORTIFY expectations reduced without executing strlen. */
#define is_constexpr(x) \
    (sizeof(int) == sizeof(*(8 ? (void *)((long)(x) * 0L) : (int *)8)))

static const char fixed_array[] = "this is 10";
static const char *const fixed_pointer = "this is 12!!";
static const char *mutable_pointer = "this is 11!";
static char mutable_array[] = "compiler thinks I might change";

int main(void)
{
    char automatic_array[80] = "Test!";
    return is_constexpr(__builtin_strlen("88888888"))
        | (is_constexpr(__builtin_strlen(fixed_array)) << 1)
        | (is_constexpr(__builtin_strlen(fixed_pointer)) << 2)
        | (is_constexpr(__builtin_strlen(mutable_pointer)) << 3)
        | (is_constexpr(__builtin_strlen(automatic_array)) << 4)
        | (is_constexpr(__builtin_strlen(mutable_array)) << 5);
}
