/* L11a: <stdatomic.h> through the target's own header (GCC's on Linux,
 * Clang's on Darwin), including zlib's once pattern and TinyCC bcheck's
 * _Atomic int counter. Single-thread values; exit 0 is the answer. */
#include <stdatomic.h>
#include <stdint.h>

typedef struct { atomic_flag begun; atomic_int done; } once_t;
static once_t made = {ATOMIC_FLAG_INIT, 0};
static int runs;
static void init(void) { runs++; }
static void once(once_t *state, void (*fn)(void))
{
    if (!atomic_load(&state->done)) {
        if (atomic_flag_test_and_set(&state->begun))
            while (!atomic_load(&state->done)) { }
        else { fn(); atomic_store(&state->done, 1); }
    }
}
static _Atomic int never_fatal;

int main(void)
{
    once(&made, init); once(&made, init);
    if (runs != 1) return 1;
    never_fatal = 0;
    atomic_fetch_add(&never_fatal, 3);
    if (never_fatal <= 0) return 2;
    atomic_int c;
    atomic_init(&c, 5);
    int e = 4;
    if (atomic_compare_exchange_strong(&c, &e, 9) || e != 5) return 3;
    while (!atomic_compare_exchange_weak_explicit(&c, &e, 9, memory_order_acq_rel,
                                                  memory_order_acquire))
        if (e != 5) return 4;
    if (atomic_exchange(&c, 1) != 9 || atomic_load_explicit(&c, memory_order_relaxed) != 1)
        return 5;
    if (atomic_fetch_sub_explicit(&c, 1, memory_order_release) != 1) return 6;
    atomic_thread_fence(memory_order_seq_cst);
    atomic_signal_fence(memory_order_acquire);
    if (!atomic_is_lock_free(&c) || ATOMIC_INT_LOCK_FREE != 2) return 7;
    _Atomic(intptr_t) ip = 8;
    if (atomic_fetch_or(&ip, 1) != 8 || ip != 9) return 8;
    atomic_flag_clear(&made.begun);
    return kill_dependency(c) != 0;
}
