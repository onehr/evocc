/* pthread_cleanup_push and pthread_cleanup_pop with cancellation (D16; Linux
   only). glibc's push macro calls __sigsetjmp_cancel, which its prototype
   marks returns_twice. Cancellation unwinds to that call, and it returns a
   second time. The macro then calls the cleanup routine with its argument,
   two unchanged automatic objects of the macro that are dead on the path of
   the first return. That path keeps fourteen temporaries live across calls.
   main returns 0, or the number of the first check that failed. */
#include <pthread.h>
#include <unistd.h>

static volatile int ready;
static volatile int handled;
volatile int sink;

__attribute__((noinline)) static int opaque(int v) { sink = v; return v * 17 + 6; }

static void release(void *argument) {
    int *values = argument;
    handled = values[0] + values[1];
}

static void *worker(void *argument) {
    int seed = *(int *)argument;
    int values[2] = {opaque(seed), opaque(seed + 1)};
    pthread_cleanup_push(release, values);
    int t0 = opaque(100), t1 = opaque(101), t2 = opaque(102), t3 = opaque(103),
        t4 = opaque(104), t5 = opaque(105), t6 = opaque(106), t7 = opaque(107),
        t8 = opaque(108), t9 = opaque(109), t10 = opaque(110), t11 = opaque(111),
        t12 = opaque(112), t13 = opaque(113);
    sink = t0 + t1 + t2 + t3 + t4 + t5 + t6 + t7 + t8 + t9 + t10 + t11 + t12
           + t13;
    ready = 1;
    for (;;)
        pause();
    pthread_cleanup_pop(0);
    return 0;
}

int main(void) {
    pthread_t thread;
    int seed = 3;
    void *result = 0;
    if (pthread_create(&thread, 0, worker, &seed) != 0)
        return 1;
    while (!ready)
        usleep(1000);
    if (pthread_cancel(thread) != 0)
        return 2;
    if (pthread_join(thread, &result) != 0)
        return 3;
    if (result != PTHREAD_CANCELED)
        return 4;
    if (handled != (3 * 17 + 6) + (4 * 17 + 6))
        return 5;
    return 0;
}
