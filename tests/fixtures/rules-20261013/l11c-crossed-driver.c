/* L11c: driver for l11c-crossed-worker.c. Exit 0 when no update is lost. */
#include <pthread.h>
#include <stdio.h>
struct __attribute__((aligned(16))) w16 { long count, check; };
struct w12 { int count, check, spare; };
struct __attribute__((aligned(8))) w8 { int count, check; };
struct w16 c16;
struct w12 c12;
struct w8 c8;
void work_a(int);
void work_b(int);
enum { N = 200000 };
static void *run_a(void *unused) { (void)unused; work_a(N); return 0; }
static void *run_b(void *unused) { (void)unused; work_b(N); return 0; }

int main(void)
{
    pthread_t a, b;
    if (pthread_create(&a, 0, run_a, 0) || pthread_create(&b, 0, run_b, 0)) return 10;
    pthread_join(a, 0);
    pthread_join(b, 0);
    printf("c16=%ld/%ld c12=%d/%d c8=%d/%d\n", c16.count, c16.check, c12.count,
           c12.check, c8.count, c8.check);
    return c16.count != 2 * N || c16.check != 4 * N || c12.count != 2 * N
        || c12.check != 4 * N || c8.count != 2 * N || c8.check != 4 * N;
}
