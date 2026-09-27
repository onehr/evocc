/* Bounded shell transport repair; not a complete wordexp parser test. */
#define _XOPEN_SOURCE 700
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <wordexp.h>

struct input {const char *s; int flags, error, noisy; const char *words;};
static const struct input inputs[] = {
    {"'unterminated", 0, WRDE_SYNTAX, 0, ""},
    {"'unterminated", WRDE_SHOWERR, WRDE_SYNTAX, 1, ""},
    {"\"unterminated", 0, WRDE_SYNTAX, 0, ""},
    {"\"unterminated", WRDE_SHOWERR, WRDE_SYNTAX, 1, ""},
    {"$(printf diagnostic >&2)", 0, 0, 0, ""},
    {"$(printf diagnostic >&2)", WRDE_SHOWERR, 0, 1, ""},
    {"$(printf diagnostic >&2; printf result)", 0, 0, 0, "result|"},
    {"$(printf diagnostic >&2; printf result)", WRDE_SHOWERR, 0, 1, "result|"},
    {"$1", 0, 0, 0, ""}, {"$2", 0, 0, 0, ""},
    {"$*", 0, 0, 0, ""}, {"$@", 0, 0, 0, ""},
    {"$#", 0, 0, 0, "0|"}, {"\"$@\"", 0, 0, 0, ""},
    {"one 'two three'", 0, 0, 0, "one|two three|"},
    {"${EVO_WORD_VALUE}", 0, 0, 0, "bar|baz|"},
};

int main(void)
{
    FILE *capture = tmpfile();
    int saved = dup(2);
    if (!capture || saved < 0 || dup2(fileno(capture), 2) < 0 ||
        setenv("EVO_WORD_VALUE", "bar baz", 1)) return 2;
    int checks = 0, failures = 0;
    for (unsigned i = 0; i < sizeof inputs / sizeof *inputs; ++i) {
        const struct input *p = inputs+i;
        wordexp_t we = {0};
        int result = wordexp(p->s, &we, p->flags);
        int noisy = lseek(fileno(capture), 0, SEEK_END) > 0;
        if (ftruncate(fileno(capture), 0) || lseek(fileno(capture), 0, SEEK_SET) < 0) return 3;
        char words[256] = {0};
        size_t n = 0;
        if (!result) {
            for (size_t j = 0; j < we.we_wordc; ++j) {
                size_t length = strlen(we.we_wordv[j]);
                if (n+length+1 >= sizeof words) return 4;
                memcpy(words+n, we.we_wordv[j], length);
                n += length; words[n++] = '|'; words[n] = 0;
            }
            wordfree(&we);
        }
        checks += 3;
        if (result != p->error) { ++failures; printf("return mismatch case %u\n", i); }
        if (noisy != p->noisy) { ++failures; printf("stderr mismatch case %u\n", i); }
        if (strcmp(words, p->words)) { ++failures; printf("words mismatch case %u: %s\n", i, words); }
    }
    if (dup2(saved, 2) < 0) return 5;
    close(saved); fclose(capture);
    printf("checks=%d failures=%d\n", checks, failures);
    return failures != 0;
}
