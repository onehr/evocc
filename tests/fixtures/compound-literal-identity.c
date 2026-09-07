struct S { int value; };

static int repeated(void) {
    struct S *previous = 0, *current = 0;
    int count = 0;
again:
    previous = current;
    current = &(struct S){++count};
    if (count < 3) goto again;
    return current != previous || previous->value != 3;
}

static int recursive(int depth, struct S *parent) {
    struct S *current = &(struct S){depth};
    if (parent && parent == current) return 1;
    if (depth && recursive(depth - 1, current)) return 2;
    return current->value != depth;
}

static int loop(void) {
    int sum = 0;
    for (int i = 0; i < 3; ++i) {
        int count = 0;
        struct S *previous = 0, *current = 0;
again:
        previous = current;
        current = &(struct S){i + ++count};
        if (count < 2) goto again;
        if (previous != current) return 1;
        sum += previous->value;
    }
    return sum != 9;
}

int main(void) {
    struct S *a = &(struct S){1}, *b = &(struct S){1};
    return a == b || repeated() || recursive(3, 0) || loop();
}
