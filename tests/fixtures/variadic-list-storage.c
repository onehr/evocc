/* Adjacent opaque cursors have the target's size and independent positions. */
struct lists {
    unsigned long before;
    __builtin_va_list item[2];
    unsigned long after;
};

static int sum_twice(int n, ...)
{
    struct lists s;
    s.before = 23;
    s.after = 29;
    __builtin_va_start(s.item[0], n);
    __builtin_va_copy(s.item[1], s.item[0]);
    int first = 0;
    int second = 0;
    for (int i = 0; i < n; ++i) {
        first += __builtin_va_arg(s.item[0], int);
        second += __builtin_va_arg(s.item[1], int);
    }
    __builtin_va_end(s.item[0]);
    __builtin_va_end(s.item[1]);
    return first == 37 && second == 37 && s.before == 23 && s.after == 29;
}

int main(void)
{
    return !sum_twice(3, 7, 11, 19);
}
