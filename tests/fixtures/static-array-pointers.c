static int calls;

static int view(int n, int *data) {
    int width = n;
    static int (*p)[n++];
    static int (*slots[2])[n++];
    ++calls;
    if (n != width + 2 || sizeof(*p) != 4UL * width ||
        sizeof(*slots[0]) != 4UL * (width + 1)) return 1;
    if (calls == 1 && (p != 0 || slots[0] != 0 || slots[1] != 0)) return 2;
    p = (void *)data;
    slots[0] = (void *)data;
    slots[1] = slots[0];
    n = 99;
    p[1][0] = 17;
    slots[1][1][0] = 23;
    return data[width] != 17 || data[width + 1] != 23;
}

static __attribute__((always_inline)) inline int retained(int n, int *data) {
    static int (*p)[n];
    static int visits;
    p = (void *)data;
    p[1][0] = ++visits;
    return p[1][0];
}

int main(void) {
    int data[32] = {0};
    if (view(3, data) || view(5, data) || calls != 2) return 1;
    if (retained(2, data) != 1 || retained(8, data) != 2) return 2;
    return 0;
}
