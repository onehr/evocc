/* L11 inventory probes: deliberately outside the current scalar load/store
 * and legacy __sync contract. Each CASE is an independent defined workload;
 * a reference PASS must not turn an EvoCC refusal into an implemented feature. */
#ifndef CASE
#define CASE 0
#endif

#if CASE == 0
static _Atomic unsigned value = 3;
int main(void) {
    value += 4;
    return value != 7;
}
#elif CASE == 1
static unsigned value = 3;
int main(void) {
    unsigned expected = 7;
    if (__atomic_fetch_add(&value, 4, 5) != 3) return 1;
    if (!__atomic_compare_exchange_n(&value, &expected, 9, 0, 5, 2)) return 2;
    if (__atomic_exchange_n(&value, 11, 5) != 9) return 3;
    return value != 11;
}
#elif CASE == 2
struct __attribute__((aligned(8))) pair { unsigned a, b; };
static struct pair source = {3, 7};
int main(void) {
    struct pair snapshot, next = {11, 13};
    __atomic_load(&source, &snapshot, 0);
    if (snapshot.a != 3 || snapshot.b != 7) return 1;
    __atomic_store(&source, &next, 3);
    __atomic_load(&source, &snapshot, 2);
    return snapshot.a != 11 || snapshot.b != 13;
}
#endif
