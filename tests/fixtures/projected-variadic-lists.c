typedef __builtin_va_list argument_list;

struct holder { int guard; argument_list ap; };
int lookups;
int indices;

struct holder *locate(struct holder *p) {
    lookups++;
    return p;
}

int forward(argument_list ap) {
    int first = __builtin_va_arg(ap, int);
    return first + 10 * __builtin_va_arg(ap, int);
}

int direct_member(int n, ...) {
    struct holder h;
    __builtin_va_start(h.ap, n);
    int value = forward(h.ap);
    int original = __builtin_va_arg(h.ap, int);
    __builtin_va_end(h.ap);
    return value + 100 * original;
}

int pointer_member(int n, ...) {
    struct holder h;
    struct holder *p = &h;
    __builtin_va_start(p->ap, n);
    int value = forward(locate(p)->ap);
    int original = __builtin_va_arg(p->ap, int);
    __builtin_va_end(p->ap);
    return value + 100 * original;
}

int array_element(int n, ...) {
    argument_list lists[2];
    __builtin_va_start(lists[0], n);
    int value = forward(lists[indices++]);
    int original = __builtin_va_arg(lists[0], int);
    __builtin_va_end(lists[0]);
    return value + 100 * original;
}

int main(void) {
    if (direct_member(2, 7, 9) != 797) return 1;
    if (pointer_member(2, 3, 4) != 343 || lookups != 1) return 2;
    if (array_element(2, 5, 6) != 565 || indices != 1) return 3;
    return 0;
}
