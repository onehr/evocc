union rcu_special {
    struct { unsigned char blocked, need_qs, exp_hint, need_mb; } b;
    unsigned s;
};
struct task { union rcu_special trc_reader_special; };

__attribute__((noinline))
union rcu_special snapshot(const volatile union rcu_special *p) {
    return *p;
}

__attribute__((noinline))
void publish(volatile union rcu_special *p, const union rcu_special *q) {
    *p = *q;
}

static union rcu_special read_task(struct task *t) {
    /* Frozen tasks.h:1992, CONFIG_LTO_NONE + asm-generic/rwonce.h. */
    union rcu_special trs = ({
        _Static_assert(sizeof(t->trc_reader_special) == 4, "access size");
        (*(const volatile __typeof__(t->trc_reader_special) *)&t->trc_reader_special);
    });
    return trs;
}

int main(void) {
    struct task task = { { .s = 0x44332211 } };
    union rcu_special a = snapshot(&task.trc_reader_special);
    if (a.s != 0x44332211) return 1;
    union rcu_special b = read_task(&task);
    if (b.s != a.s) return 2;
    union rcu_special c;
    publish(&c, &b);
    if (c.s != a.s) return 3;
    int x = 19;
    union P { int *p; unsigned long bits; } source = { &x };
    union P copied = *(volatile union P *)&source;
    if (*copied.p != 19) return 4;
    return 0;
}
