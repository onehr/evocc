/* Fault injection into the separately compiled wordexp source only. */
#define _GNU_SOURCE
#include <errno.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <wordexp.h>

static uintptr_t owned[128];
static unsigned read_calls, grow_calls, fail_read, fail_grow, fail_empty;
static unsigned injected, bad_free, checks, failures;

static void forget(uintptr_t address)
{
    if (!address) return;
    for (unsigned i = 0; i < 128; ++i) if (owned[i] == address) {
        owned[i] = 0;
        return;
    }
    ++bad_free;
}

static void remember(void *pointer)
{
    if (!pointer) return;
    for (unsigned i = 0; i < 128; ++i) if (!owned[i]) {
        owned[i] = (uintptr_t)pointer;
        return;
    }
    abort();
}

ssize_t evo_getdelim(char **buffer, size_t *size, int delimiter, FILE *stream)
{
    ssize_t result = getdelim(buffer, size, delimiter, stream);
    remember(*buffer); /* A -1 result can still leave caller-owned storage. */
    if (++read_calls == fail_read) {
        ++injected;
        errno = ENOMEM;
        return -1;
    }
    return result;
}

void *evo_realloc(void *pointer, size_t size)
{
    if (++grow_calls == fail_grow) {
        ++injected;
        errno = ENOMEM;
        return 0;
    }
    uintptr_t previous = (uintptr_t)pointer;
    void *result = realloc(pointer, size);
    if (result) { forget(previous); remember(result); }
    return result;
}

void *evo_calloc(size_t count, size_t size)
{
    if (fail_empty) { ++injected; errno = ENOMEM; return 0; }
    void *result = calloc(count, size);
    remember(result);
    return result;
}

void evo_free(void *pointer)
{
    forget((uintptr_t)pointer);
    free(pointer);
}

static void check(int ok, unsigned mode, unsigned flags, const char *name)
{
    ++checks;
    if (!ok) { ++failures; printf("mode=%u flags=%u %s\n", mode, flags, name); }
}

int main(void)
{
    const char *input = "a b c d e f g h i j k l m n o p";
    for (unsigned mode = 0; mode < 6; ++mode) for (unsigned variant = 0; variant < 4; ++variant) {
        unsigned append = variant & 1, offset = variant & 2 ? 3 : 0;
        unsigned flags = WRDE_NOCMD | (append ? WRDE_APPEND : 0) |
                         (offset ? WRDE_DOOFFS : 0);
        wordexp_t words = {0};
        words.we_offs = offset;
        if (append && wordexp("old", &words, flags & ~WRDE_APPEND)) return 2;
        read_calls = grow_calls = injected = bad_free = 0;
        fail_read = mode == 1 ? 1 : mode == 2 ? 2 : 0;
        fail_grow = mode == 3 ? 1 : mode == 4 ? 2 : 0;
        fail_empty = mode == 5;
        int result = wordexp(mode == 5 ? "" : input, &words, flags);
        unsigned wants_failure = mode != 0 && !(mode == 5 && append);
        unsigned count = mode == 0 ? 16 + append : append;
        if (mode == 4) {
            unsigned initial_capacity = append ? offset + 2 : 0;
            count = initial_capacity + initial_capacity / 2 + 9 - offset;
        }
        check(result == (wants_failure ? WRDE_NOSPACE : 0), mode, flags, "status");
        check(words.we_wordc == count, mode, flags, "partial-count");
        check(words.we_offs == offset, mode, flags, "offset");
        check(words.we_wordv != 0 || (wants_failure && !count), mode, flags, "vector");
        int shape = 1, content = 1;
        if (words.we_wordv) {
            for (unsigned i = 0; i < offset; ++i) shape &= words.we_wordv[i] == 0;
            shape &= words.we_wordv[offset + words.we_wordc] == 0;
            for (unsigned i = 0; i < words.we_wordc; ++i) {
                char expected[] = {(char)('a' + i - append), 0};
                content &= !strcmp(words.we_wordv[offset+i], append && !i ? "old" : expected);
            }
        }
        check(shape && content, mode, flags, "partial-content-and-terminators");
        check(injected == wants_failure, mode, flags, "fault-executed");
        wordfree(&words);
        unsigned leaked = 0;
        for (unsigned i = 0; i < 128; ++i) leaked += owned[i] != 0;
        check(leaked == 0, mode, flags, "owned-allocations-leaked");
        check(bad_free == 0, mode, flags, "foreign-or-double-free");
        /* Dispose leaks after recording RED so every row starts independently. */
        for (unsigned i = 0; i < 128; ++i) if (owned[i]) {
            free((void *)owned[i]); owned[i] = 0;
        }
        fail_read = fail_grow = fail_empty = 0;
    }
    printf("checks=%u failures=%u\n", checks, failures);
    return failures != 0;
}
