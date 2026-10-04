unsigned loop_branch_tick(unsigned x) { return x ? x - 1U : 3U; }

unsigned loop_branch(unsigned x, unsigned n) {
    while (n) {
        x = loop_branch_tick(x);
        --n;
    }
    return x;
}

int loop_checked_tick(int x) { return x + 1; }

int loop_checked(int x, unsigned n) {
    while (n) {
        x = loop_checked_tick(x);
        --n;
    }
    return x;
}

unsigned loop_forever(unsigned x) {
    for (;;) {
        x = loop_branch_tick(x);
    }
    return x;
}
