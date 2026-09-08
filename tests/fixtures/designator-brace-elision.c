union Mutex { int lock; };
struct Inner { int a, b; };
struct Outer { int head; struct Inner inner; int tail; };
int main(void) {
    struct { int c; union Mutex m; } original = { .m = 0 };
    struct Outer first = { .inner = 1, 2, 3 };
    struct Outer second = { .inner = 4, .tail = 8 };
    return original.c || original.m.lock || first.head || first.inner.a != 1
        || first.inner.b != 2 || first.tail != 3 || second.inner.a != 4
        || second.inner.b || second.tail != 8;
}
