struct pair { long first; long second; };
long first(int, long value) { return value; }
long last(long value, int) { return value; }
int all(int, int) { return 7; }
int record(struct pair, int value) { return value; }
int main(void) {
    int evaluated = 0;
    long (*call)(int, long) = first;
    struct pair p = {11, 22};
    if (call(evaluated++, 37) != 37 || evaluated != 1) return 1;
    if (last(42, evaluated++) != 42 || evaluated != 2) return 2;
    if (all(evaluated++, 0) != 7 || evaluated != 3) return 3;
    if (record(p, 9) != 9) return 4;
    return 0;
}
