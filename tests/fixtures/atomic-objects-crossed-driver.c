/* L11a: run two builds of atomic-objects-crossed-worker.c in two threads on
 * the object of atomic-objects-crossed-object.c. Build with -DFIRST=name
 * -DSECOND=name; this driver is built by the reference compiler of the
 * target. Every unit must agree on the layout. The exit status is 0 when
 * every check passes. A pass shows no lost update and no torn value in these
 * runs; it is not a memory-model proof. */
#include <pthread.h>
#include <stdio.h>
#include "atomic-objects-crossed.h"

void CAT(FIRST, layout)(long *);
void CAT(SECOND, layout)(long *);
void CAT(FIRST, count)(struct holder *, int);
void CAT(SECOND, count)(struct holder *, int);
int CAT(FIRST, values)(void);
int CAT(SECOND, values)(void);
void object_layout(long *);

static int ready;

static void *first(void *unused)
{
    (void)unused;
    __atomic_add_fetch(&ready, 1, 5);
    while (__atomic_load_n(&ready, 5) < 2) { }
    CAT(FIRST, count)(&shared, N);
    return 0;
}

static void *second(void *unused)
{
    (void)unused;
    __atomic_add_fetch(&ready, 1, 5);
    while (__atomic_load_n(&ready, 5) < 2) { }
    CAT(SECOND, count)(&shared, N);
    return 0;
}

int main(void)
{
    long mine[LAYOUT_WORDS], one[LAYOUT_WORDS], two[LAYOUT_WORDS], object[LAYOUT_WORDS];
    int failures = 0;
    LAYOUT(mine);
    CAT(FIRST, layout)(one);
    CAT(SECOND, layout)(two);
    object_layout(object);
    for (int i = 0; i < LAYOUT_WORDS; i++) {
        if (one[i] != mine[i] || two[i] != mine[i] || object[i] != mine[i]) {
            printf("layout %d: %ld %ld %ld reference %ld\n", i, one[i], two[i], object[i],
                   mine[i]);
            failures++;
        }
    }
    if (CAT(FIRST, values)() || CAT(SECOND, values)()) {
        printf("values %d %d\n", CAT(FIRST, values)(), CAT(SECOND, values)());
        failures++;
    }
    pthread_t a, b;
    if (pthread_create(&a, 0, first, 0) || pthread_create(&b, 0, second, 0)) return 10;
    pthread_join(a, 0);
    pthread_join(b, 0);
    unsigned product = 1;
    for (int i = 0; i < 2 * N; i++) product *= 3;
    signed char wrap = (signed char)(unsigned char)(2 * N);
    if (shared.counter != 2 * N || shared.wide != 2 * N || shared.cursor != &cells[2 * N]
        || shared.product != product || shared.small != (unsigned char)(2 * N)
        || shared.wrap != wrap || !shared.flag) {
        printf("values after threads: counter=%u wide=%ld cursor=%ld product=%u small=%u wrap=%d\n",
               shared.counter, shared.wide, (long)(shared.cursor - cells), shared.product,
               shared.small, shared.wrap);
        failures++;
    }
    printf("failures=%d counter=%u\n", failures, shared.counter);
    return failures != 0;
}
