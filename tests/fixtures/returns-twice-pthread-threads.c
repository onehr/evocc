/* glibc's <pthread.h> declares __sigsetjmp_cancel with returns_twice. The
   prototype alone must not refuse the unit. This program uses no
   cancellation macro, so it never names that function. */
#include <pthread.h>

static void *worker(void *argument) {
    int *value = argument;
    *value += 41;
    return value;
}

int spawn(void) {
    pthread_t thread;
    int value = 1;
    void *result = 0;
    if (pthread_create(&thread, 0, worker, &value) != 0)
        return 1;
    if (pthread_join(thread, &result) != 0)
        return 2;
    return result == &value && value == 42 ? 0 : 3;
}

int main(void) {
#ifdef SPAWN
    return spawn();
#else
    return 0;
#endif
}
