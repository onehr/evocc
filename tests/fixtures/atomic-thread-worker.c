/* Compile each side separately to test cross-compiler publication and SC. */
struct shared { int flag, payload, x, y, a, b; };
#ifdef SIDE_A
void operation_a(struct shared *s, int phase) {
    if (phase == 0) {
        s->payload = 42;
        __atomic_store_n(&s->flag, 1, __ATOMIC_RELEASE);
    } else {
        __atomic_store_n(&s->x, 1, __ATOMIC_SEQ_CST);
        s->a = __atomic_load_n(&s->y, __ATOMIC_SEQ_CST);
    }
}
#else
void operation_b(struct shared *s, int phase) {
    if (phase == 0) {
        while (!__atomic_load_n(&s->flag, __ATOMIC_ACQUIRE)) { }
        s->b = s->payload;
    } else {
        __atomic_store_n(&s->y, 1, __ATOMIC_SEQ_CST);
        s->b = __atomic_load_n(&s->x, __ATOMIC_SEQ_CST);
    }
}
#endif
