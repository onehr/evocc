/* Bounded checks of the patched wordexp; not a complete wordexp test.
 * The unchanged libc-test functional/wordexp program is the full oracle.
 * Inputs 0-15 check shell transport. The others check the contracts that
 * the original program does not fix: dollar-single-quote rewriting,
 * WRDE_UNDEF classification, and recorded boundaries of this design. */
#define _XOPEN_SOURCE 700
#include <fcntl.h>
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
    /* POSIX.1-2024 dollar-single-quotes at the top level. */
    {"$'a\\tb'", 0, 0, 0, "a\tb|"},
    {"$'\\x41\\101\\cA\\e\\c\\\\\\c?'", 0, 0, 0, "AA\001\033\034\177|"},
    {"$'it\\'s' $'\\\"\\\\'", 0, 0, 0, "it's|\"\\|"},
    {"$'a b' $'*' ~$'x'", 0, 0, 0, "a b|*|~x|"},
    {"x$'\\n'y", 0, 0, 0, "x\ny|"},
    {"$'ab\\0cd\\x41'e", 0, 0, 0, "abe|"},
    {"$'$(printf bad)'", WRDE_NOCMD, 0, 0, "$(printf bad)|"},
    {"\"$'x'\"", 0, 0, 0, "$'x'|"},
    {"$'\\q'", 0, WRDE_SYNTAX, 0, ""},
    {"$'\\x'", 0, WRDE_SYNTAX, 0, ""},
    {"$'\\x414'", 0, WRDE_SYNTAX, 0, ""},
    {"$'\\400'", 0, WRDE_SYNTAX, 0, ""},
    {"$'\\c@'", 0, WRDE_SYNTAX, 0, ""},
    {"$'abc", 0, WRDE_SYNTAX, 0, ""},
    {"${X-$'a'}", WRDE_NOCMD, WRDE_SYNTAX, 0, ""},
    /* Recorded boundary: without WRDE_NOCMD, text after a construct the
       scanner does not follow keeps the pinned shell's meaning. */
    {"$'a' $(printf b) $'c'", 0, 0, 0, "a|b|$c|"},
    {"${X-$'a'}", 0, 0, 0, "$a|"},
    /* WRDE_UNDEF: only a failure caused by set -u is WRDE_BADVAL. */
    {"x $EVO_UNSET y", WRDE_UNDEF, WRDE_BADVAL, 0, ""},
    {"$EVO_UNSET", WRDE_UNDEF|WRDE_SHOWERR, WRDE_BADVAL, 1, ""},
    {"$((1/0))", WRDE_UNDEF, WRDE_SYNTAX, 0, ""},
    {"${EVO_UNSET:?gone}", WRDE_UNDEF, WRDE_SYNTAX, 0, ""},
    {"$'x' $EVO_UNSET", WRDE_UNDEF, WRDE_BADVAL, 0, ""},
    {"$(printf a) $EVO_UNSET", WRDE_UNDEF, WRDE_BADVAL, 0, ""},
    {"$(printf a) )", WRDE_UNDEF, WRDE_SYNTAX, 0, ""},
    {"${EVO_UNSET-x} $(printf a)", WRDE_UNDEF, 0, 0, "x|a|"},
    /* Recorded boundary: with a command substitution, the failure is not
       repeated to classify it, so this arithmetic error is WRDE_BADVAL. */
    {"$(printf a) $((1/0))", WRDE_UNDEF, WRDE_BADVAL, 0, ""},
};

static int checks, failures;

static void check(int ok, const char *what, unsigned i)
{
    ++checks;
    if (!ok) { ++failures; printf("%s mismatch case %u\n", what, i); }
}

/* Each command substitution must run once, even when classified. */
static void side_effects(int capture, const char *dir, unsigned i)
{
    char path[128], input[256], data[16];
    static const char *const tails[] = {" $EVO_UNSET", " $EVO_UNSET )"};
    static const int want[] = {WRDE_BADVAL, WRDE_SYNTAX};
    static const char *const contents[] = {"x", ""};
    for (unsigned j = 0; j < 2; ++j, ++i) {
        snprintf(path, sizeof path, "%s/effect%u", dir, j);
        snprintf(input, sizeof input, "$(printf x >> %s)%s", path, tails[j]);
        wordexp_t we = {0};
        int result = wordexp(input, &we, WRDE_UNDEF);
        int fd = open(path, O_RDONLY);
        ssize_t n = fd < 0 ? 0 : read(fd, data, sizeof data - 1);
        data[n < 0 ? 0 : n] = 0;
        if (fd >= 0) close(fd);
        unlink(path);
        check(result == want[j], "return", i);
        check(!strcmp(data, contents[j]), "effect", i);
        check(lseek(capture, 0, SEEK_END) == 0, "stderr", i);
        if (!result) wordfree(&we);
    }
}

int main(void)
{
    FILE *capture = tmpfile();
    int saved = dup(2);
    char dir[] = "/tmp/evo-wordexpXXXXXX";
    if (!capture || saved < 0 || dup2(fileno(capture), 2) < 0 ||
        setenv("EVO_WORD_VALUE", "bar baz", 1) || unsetenv("X") ||
        unsetenv("EVO_UNSET") || !mkdtemp(dir) || chdir(dir)) return 2;
    /* An unquoted * would match this file. */
    int file = open("glob", O_WRONLY|O_CREAT|O_EXCL, 0600);
    if (file < 0 || close(file)) return 2;
    unsigned i;
    for (i = 0; i < sizeof inputs / sizeof *inputs; ++i) {
        const struct input *p = inputs+i;
        wordexp_t we = {0};
        int result = wordexp(p->s, &we, p->flags);
        off_t size = lseek(fileno(capture), 0, SEEK_END);
        char text[256] = {0};
        if (size > 0 && pread(fileno(capture), text, sizeof text - 1, 0) < 0) return 3;
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
        check(result == p->error, "return", i);
        check((size > 0) == p->noisy, "stderr", i);
        check(!strcmp(words, p->words), "words", i);
        /* A visible WRDE_UNDEF error is written once, not once per run. */
        if (p->noisy && (p->flags & WRDE_UNDEF))
            check(strchr(text, '\n') == text + strlen(text) - 1, "repeat", i);
    }
    side_effects(fileno(capture), dir, i);
    if (dup2(saved, 2) < 0 || unlink("glob") || chdir("/") || rmdir(dir)) return 5;
    close(saved); fclose(capture);
    printf("checks=%d failures=%d\n", checks, failures);
    return failures != 0;
}
