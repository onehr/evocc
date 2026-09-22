#include <pthread.h>
extern void *increment(void *);
extern unsigned long read_counter(void);
int main(void) {
    pthread_t a, b;
    if (pthread_create(&a, 0, increment, 0) || pthread_create(&b, 0, increment, 0)) return 2;
    if (pthread_join(a, 0) || pthread_join(b, 0)) return 3;
    return read_counter() != 20000;
}
