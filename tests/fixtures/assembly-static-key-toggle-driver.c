/* Linux/AArch64 native driver, built by the fixed oracle, not EvoCC. */
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <sys/mman.h>
#include <unistd.h>

struct key { int enabled; long entries; };
struct entry { int32_t code, target; intptr_t key; };
extern struct entry __start___jump_table[], __stop___jump_table[];
extern struct key key_false, key_true;
extern int *root;
extern int read_if_enabled(int, int);

static void patch(int first, int second)
{
    unsigned seen = 0;
    for (struct entry *p = __start___jump_table; p != __stop___jump_table; ++p) {
        uintptr_t tagged = (uintptr_t)&p->key + p->key;
        uintptr_t key = tagged & ~(uintptr_t)1;
        assert(key == (uintptr_t)&key_false || key == (uintptr_t)&key_true);
        int taken = key == (uintptr_t)&key_false ? first : second;
        assert((tagged & 1) == (key == (uintptr_t)&key_true));
        seen |= key == (uintptr_t)&key_false ? 1 : 2;
        uintptr_t code = (uintptr_t)&p->code + p->code;
        uintptr_t target = (uintptr_t)&p->target + p->target;
        long pagesize = sysconf(_SC_PAGESIZE);
        assert(pagesize > 0);
        assert(!mprotect((void *)(code & ~(uintptr_t)(pagesize - 1)),
                         pagesize, PROT_READ | PROT_WRITE | PROT_EXEC));
        intptr_t delta = (intptr_t)target - (intptr_t)code;
        assert(!(delta & 3) && delta >= -(1L << 27) && delta < (1L << 27));
        *(uint32_t *)code = taken ? 0x14000000u | ((delta / 4) & 0x03ffffffu)
                                 : 0xd503201fu;
        __builtin___clear_cache((char *)code, (char *)code + 4);
    }
    assert(seen == 3);
}

int main(void)
{
    /* The disabled path must not access the NULL root. */
    assert(read_if_enabled(2, 5) == 48);
    int value = 19;
    for (int i = 0; i != 100; ++i) {
        root = &value;
        patch(1, 0);
        assert(read_if_enabled(i, 5) == i + 24);
        patch(0, 1);
        root = 0;
        assert(read_if_enabled(i, 5) == 4 * i + 40);
        patch(0, 0);
        assert(read_if_enabled(i, 5) == i + 5);
    }
    puts("static-key toggle: PASS");
    return 0;
}
