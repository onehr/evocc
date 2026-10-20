/* pthread_cleanup_push expands to a call of __sigsetjmp_cancel, which glibc
   declares with returns_twice. The call is a returns-twice call (EvoIR 3.10);
   the interpreters refuse it as an external call (E3039). With UNUSED, the
   macros appear only in a static inline function that the unit never
   retains. */
#include <pthread.h>

static void release(void *argument) {
    *(int *)argument += 1;
}

#ifdef UNUSED
static inline
#endif
int guarded(void) {
    int value = 0;
    pthread_cleanup_push(release, &value);
    value = 1;
    pthread_cleanup_pop(1);
    return value;
}

int main(void) {
#ifdef UNUSED
    return 0;
#else
    return guarded() != 2;
#endif
}
