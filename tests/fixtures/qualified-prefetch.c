static volatile int values[3];
static volatile int *volatile pointer = values;
static const volatile int read_only;

int main(void) {
    __builtin_prefetch(pointer++);
    __builtin_prefetch(&values[2], 1, 3);
    __builtin_prefetch(&read_only, 0, 0);
    return pointer != &values[1];
}
