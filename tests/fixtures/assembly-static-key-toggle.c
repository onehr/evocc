/* Reduced from arm64's jump_label.h, without kernel or test infrastructure. */
struct key { int enabled; long entries; };
struct key key_false, key_true;
int *root;

#define ENTRY " .pushsection __jump_table, \"aw\"\n" \
              " .balign 8\n .long 1b - ., %l[yes] - .\n" \
              " .quad %c0 - .\n .popsection\n"

static __attribute__((always_inline)) inline _Bool
branch_false(struct key *key, _Bool polarity)
{
    char *tagged = (char *)key + polarity;
    asm goto("1: nop\n" ENTRY : : "i"(tagged) : : yes);
    return 0;
yes:
    return 1;
}

static __attribute__((always_inline)) inline _Bool
branch_true(struct key *key, _Bool polarity)
{
    char *tagged = (char *)key + polarity;
    asm goto("1: b %l[yes]\n" ENTRY : : "i"(tagged) : : yes);
    return 0;
yes:
    return 1;
}

int read_if_enabled(int a, int b)
{
    int result = a + b;
    if (branch_false(&key_false, 0))
        result += *root;
    if (branch_true(&key_true, 1))
        result += a * 3 + b * 7;
    return result;
}
