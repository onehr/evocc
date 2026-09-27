/* B08d: observe the unchanged pinned libc and its shell boundary.
 * Successful probe execution is NOT a successful upstream wordexp test. */
#define _XOPEN_SOURCE 700
#include <stdio.h>
#include <stdlib.h>
#include <wordexp.h>

struct input { const char *name, *text; int flags; };
static const struct input cases[] = {
    {"plain", "one two", 0},
    {"variable", "${EVO_WORD_VALUE}", 0},
    {"variable-nocmd", "${EVO_WORD_VALUE}", WRDE_NOCMD},
    {"unset", "$EVO_WORD_UNSET", 0},
    {"unset-undef", "$EVO_WORD_UNSET", WRDE_UNDEF},
    {"syntax-quiet", "'unterminated", 0},
    {"syntax-show", "'unterminated", WRDE_SHOWERR},
    {"command-quiet", "$(printf diagnostic >&2)", 0},
    {"command-show", "$(printf diagnostic >&2)", WRDE_SHOWERR},
    {"command-nocmd", "$(printf x)", WRDE_NOCMD},
    {"positional", "$1", 0},
    {"special-count", "$#", 0},
    {"dollar-quote", "$'x'", 0},
    {"comment", "one # two", 0},
};

int main(int argc, char **argv)
{
    if (argc != 2) return 100;
    unsigned selected = (unsigned)atoi(argv[1]);
    if (selected >= sizeof cases / sizeof cases[0]) return 101;
    if (setenv("EVO_WORD_VALUE", "bar baz", 1) || unsetenv("EVO_WORD_UNSET"))
        return 102;
    const struct input *c = &cases[selected];
    wordexp_t words = {0};
    int error = wordexp(c->text, &words, c->flags);
    const char *name = error == 0 ? "OK" :
        error == WRDE_BADCHAR ? "BADCHAR" : error == WRDE_BADVAL ? "BADVAL" :
        error == WRDE_CMDSUB ? "CMDSUB" : error == WRDE_SYNTAX ? "SYNTAX" :
        error == WRDE_NOSPACE ? "NOSPACE" : "UNKNOWN";
    printf("%s: %s", c->name, name);
    if (!error) {
        printf(" count=%zu", words.we_wordc);
        for (size_t i = 0; i < words.we_wordc; ++i) {
            printf(" [");
            const unsigned char *p = (const unsigned char *)words.we_wordv[i];
            while (*p) printf("%02x", *p++);
            printf("]");
        }
        wordfree(&words);
    }
    printf("\n");
    return 0;
}
