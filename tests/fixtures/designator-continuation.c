struct Inner { int a, b; };
struct Outer { int head; struct Inner inner; int tail; };
int main(void) {
    struct Outer first = { .inner.a = 11, 22, 33 };
    struct Outer second = { .inner = { 4 }, 5 };
    struct Outer third = { .inner.b = 7, .inner.a = 6, 8, 9 };
    int matrix[2][3] = { [0][1] = 5, 6, 7 };
    struct Inner inferred[] = { [1].a = 4, 5, 6, 7 };
    if (first.head || first.inner.a != 11 || first.inner.b != 22 || first.tail != 33) return 1;
    if (second.head || second.inner.a != 4 || second.inner.b || second.tail != 5) return 2;
    if (third.inner.a != 6 || third.inner.b != 8 || third.tail != 9) return 3;
    if (matrix[0][0] || matrix[0][1] != 5 || matrix[0][2] != 6 || matrix[1][0] != 7) return 4;
    if (sizeof(inferred) != 3 * sizeof(struct Inner)) return 5;
    if (inferred[0].a || inferred[1].a != 4 || inferred[1].b != 5 || inferred[2].a != 6 || inferred[2].b != 7) return 6;
    return 0;
}
