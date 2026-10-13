/* K07 probe: which __builtin_strlen operands are integer constant
 * expressions? The exit value is a classification bitmask. The kernel macro
 * reads its operand only inside sizeof, so no strlen call executes.
 * CASE=0 FORTIFY forms, 1 linkage and storage, 2 termination and offsets,
 * 3 representation and expression forms, 4 initializer and binding forms. */
#define is_constexpr(x) \
    (sizeof(int) == sizeof(*(8 ? (void *)((long)(x) * 0L) : (int *)8)))
#define B(n, e) (is_constexpr(__builtin_strlen(e)) << (n))

static const char array_of_10[] = "this is 10";
static const char *const unchanging_12 = "this is 12!!";
static const char *ptr_of_11 = "this is 11!";
static char array_unknown[] = "compiler thinks I might change";
const char external_definition[] = "abcd";
extern const char external_declaration[];
static const volatile char volatile_array[] = "abc";
static const char *const into_array = array_of_10 + 3;
static const char *const into_literal = "hello" + 1;
static const char embedded[] = "ab\0cd";
static const char unterminated[3] = "abc";
static const struct { int k; char s[8]; } record = { 1, "abc" };
static const char *const names[] = { "a", "bcd" };
static const char rows[2][4] = { "ab", "cde" };
static const int word = 0x00636261;
static const unsigned char bytes[] = { 'a', 'b', 0 };
static const char *const self = (const char *)&self;
static char *const unqualified_pointee = "abc";
static char *const into_mutable = array_unknown;
static const char padded[10] = "abc";
static const char tentative[4];
extern const char defined_later[];
__attribute__((weak)) const char weak_array[] = "abc";
static const char *const null_pointer = 0;
static const char *const from_wide = (const char *)L"ab";
struct kunit_like { const char *name; };
static struct kunit_like kunit_object = { "dynamic" };

int main(void)
{
    char stack[80] = "Test!";
    static const char local_static[] = "abc";
    const char automatic_const[] = "abc";
    const char *const automatic_const_pointer = "abc";
    struct kunit_like *test = &kunit_object;
    (void)stack; (void)automatic_const; (void)automatic_const_pointer; (void)test;
#if CASE == 0
    return B(0, "88888888") | B(1, array_of_10) | B(2, unchanging_12)
        | B(3, ptr_of_11) | B(4, stack) | B(5, array_unknown)
        | B(6, test->name) | B(7, array_of_10 + 2);
#elif CASE == 1
    return B(0, external_definition) | B(1, external_declaration)
        | B(2, local_static) | B(3, automatic_const)
        | B(4, automatic_const_pointer) | B(5, (const char *)volatile_array)
        | B(6, into_array) | B(7, into_literal);
#elif CASE == 2
    return B(0, embedded) | B(1, unterminated) | B(2, array_of_10 + 10)
        | B(3, array_of_10 + 11) | B(4, &array_of_10[2]) | B(5, record.s)
        | B(6, names[1]) | B(7, rows[1]);
#elif CASE == 3
    return B(0, (const char *)&word) | B(1, (const char *)bytes)
        | B(2, self) | B(3, 1 ? array_of_10 : "xyz")
        | B(4, ((const char *)array_of_10)) | B(5, ({ array_of_10; }))
        | B(6, (0, array_of_10)) | B(7, array_of_10 + word);
#elif CASE == 4
    return B(0, unqualified_pointee) | B(1, into_mutable) | B(2, padded)
        | B(3, tentative) | B(4, defined_later) | B(5, weak_array)
        | B(6, null_pointer) | B(7, from_wide);
#endif
}

const char defined_later[] = "abc";
