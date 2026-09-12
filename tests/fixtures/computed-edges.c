/* Return codes make each failed storage/edge obligation identifiable. */
static int recurse(int n) {
    int x = n;
    static void *done_address = &&done;
    if (n) x += recurse(n - 1);
    goto *done_address;
done:
    return x;
}

static int distinct(int choose) {
    int x = 5, y = 9;
    static void *targets[] = {&&left, &&right};
    goto *targets[choose];
left:
    y += x;
    goto *targets[1];
right:
    x += y;
    return x * 100 + y;
}

static int capture(int n) {
    typedef int row[n++];
    int *p = __builtin_alloca(sizeof(row));
    p[0] = n;
    goto *&&done;
done:
    return sizeof(row) + p[0];
}

static int scope(int choose) {
    int x = 0;
    static void *targets[] = {&&inside, &&done};
    goto *targets[choose];
    {
        int y;
inside:
        y = 6;
        x = y;
        goto *targets[1];
    }
done:
    return x;
}

static int resumed_expression(void) {
    int n = 0;
    void *target = &&again;
    return 3 + ({ again: if (++n < 3) goto *target; 5; });
}

static int captured_step(int n) {
    int storage[6];
    int (*p)[n] = (void *)storage;
    goto *&&done;
done:
    return (char *)(p + 1) - (char *)p;
}

int main(void) {
    if (recurse(4) != 10) return 1;
    if (distinct(0) != 1914 || distinct(1) != 1409) return 2;
    if (capture(3) != 16 || capture(5) != 26) return 3;
    if (scope(0) != 6 || scope(1) != 0) return 4;
    if (resumed_expression() != 8) return 5;
    if (captured_step(3) != 12) return 6;
    int x = 0;
    unsigned long address = (unsigned long)&&again;
again:
    if (++x < 3) goto *(void *)address;
    return x != 3;
}
