static unsigned long counter;
void *increment(void *unused) {
    (void)unused;
    for (int i = 0; i < 10000; ++i) __sync_fetch_and_add(&counter, 1);
    return 0;
}
unsigned long read_counter(void) { return __atomic_load_n(&counter, 5); }
