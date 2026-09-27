/* Two native threads synchronize before each FP operation; opposite FPCR
 * settings must stay thread-local. Subject function is compiled by EvoCC. */
#include <fenv.h>
#include <pthread.h>
#include <stdint.h>
#include <stdio.h>
extern unsigned long runtime_volatile(void);
static pthread_barrier_t barrier;
static void *worker(void *argument) {
    uintptr_t upward=(uintptr_t)argument,failed=0;
    for (unsigned i=0;i<64;++i) {
        if (fesetround(upward ? FE_UPWARD : FE_DOWNWARD) || feclearexcept(FE_ALL_EXCEPT)) failed=1;
        pthread_barrier_wait(&barrier);
        if (runtime_volatile()!=0x3ff0000000000000UL+upward
            || fetestexcept(FE_ALL_EXCEPT)!=FE_INEXACT
            || fegetround()!=(upward ? FE_UPWARD : FE_DOWNWARD)) failed=1;
        pthread_barrier_wait(&barrier);
    }
    return (void *)failed;
}
int main(void) {
    pthread_t threads[2]; void *a,*b;
    if (pthread_barrier_init(&barrier,0,2)) return 2;
    if (pthread_create(&threads[0],0,worker,(void *)0)
        || pthread_create(&threads[1],0,worker,(void *)1)) return 3;
    if (pthread_join(threads[0],&a) || pthread_join(threads[1],&b)) return 4;
    pthread_barrier_destroy(&barrier);
    printf("thread FP isolation: %s (128 synchronized operations)\n",a||b ? "FAIL" : "PASS");
    return a || b;
}
