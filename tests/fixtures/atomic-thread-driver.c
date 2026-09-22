#define _GNU_SOURCE
#include <pthread.h>
#include <stdio.h>

struct shared { int flag, payload, x, y, a, b; };
void operation_a(struct shared *, int);
void operation_b(struct shared *, int);
static struct shared state;
static pthread_barrier_t barrier;
static int phase;
enum { ROUNDS = 5000 };

static void *worker(void *which) {
    for (int i = 0; i < 2 * ROUNDS; i++) {
        pthread_barrier_wait(&barrier);
        if (which) operation_a(&state, phase);
        else operation_b(&state, phase);
        pthread_barrier_wait(&barrier);
    }
    return 0;
}

int main(void) {
    pthread_t a, b;
    int marker;
    unsigned publication_failures = 0, sc_forbidden = 0;
    if (pthread_barrier_init(&barrier, 0, 3)) return 10;
    if (pthread_create(&a, 0, worker, &marker)) return 11;
    if (pthread_create(&b, 0, worker, 0)) return 12;
    for (int i = 0; i < 2 * ROUNDS; i++) {
        phase = i % 2;
        state = (struct shared){0};
        pthread_barrier_wait(&barrier);
        pthread_barrier_wait(&barrier);
        if (!phase && state.b != 42) publication_failures++;
        if (phase && state.a == 0 && state.b == 0) sc_forbidden++;
    }
    pthread_join(a, 0);
    pthread_join(b, 0);
    pthread_barrier_destroy(&barrier);
    printf("publication=%u sc_forbidden=%u rounds=%d\n", publication_failures, sc_forbidden, ROUNDS);
    return publication_failures || sc_forbidden;
}
