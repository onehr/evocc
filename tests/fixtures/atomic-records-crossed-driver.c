/* L11c: run two builds of atomic-records-crossed-worker.c in two threads on
 * the same records. Build with -DFIRST=name -DSECOND=name. The exit status
 * is 0 when no update is lost and no record is torn. A pass is not a
 * memory-model proof. */
#include <pthread.h>
#include <stdio.h>
#include "atomic-records-crossed.h"

struct w8 c8;
struct w4 c4;
struct w2 c2;
struct w1 c1;
_Atomic struct pair cp;
struct lock guard;
long guarded;
static int ready;

void CAT(FIRST, work)(int, int);
void CAT(SECOND, work)(int, int);

static void *first(void *unused)
{
    (void)unused;
    __atomic_add_fetch(&ready, 1, 5);
    while (__atomic_load_n(&ready, 5) < 2) { }
    CAT(FIRST, work)(N, 1);
    return 0;
}

static void *second(void *unused)
{
    (void)unused;
    __atomic_add_fetch(&ready, 1, 5);
    while (__atomic_load_n(&ready, 5) < 2) { }
    CAT(SECOND, work)(N, 2);
    return 0;
}

int main(void)
{
    pthread_t a, b;
    if (pthread_create(&a, 0, first, 0) || pthread_create(&b, 0, second, 0)) return 10;
    pthread_join(a, 0);
    pthread_join(b, 0);
    struct pair p = cp;
    int failures = c8.count != 2 * N || c8.check != 4 * N
        || c4.count != (short)(2 * N) || c4.check != (short)(4 * N)
        || c2.count != (unsigned char)(2 * N) || c2.check != (unsigned char)(4 * N)
        || c1.count != (unsigned char)(2 * N) || p.count != 2 * N || p.check != 4 * N
        || guarded != 2 * N || guard.held != 0;
    printf("failures=%d c8=%d/%d c4=%d/%d c2=%d/%d c1=%d cp=%d/%d guarded=%ld\n", failures,
           c8.count, c8.check, c4.count, c4.check, c2.count, c2.check, c1.count, p.count,
           p.check, guarded);
    return failures;
}
