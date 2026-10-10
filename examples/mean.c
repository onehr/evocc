/* Return the mean of the positive values in xs. */

struct stats { int count; int total; };

static void add(struct stats *s, int x) {
    if (x > 0) { s->count += 1; s->total += x; }
}

int mean(const int *xs, int n) {
    struct stats s;
    for (int i = 0; i < n; i++) add(&s, xs[i]);
    return s.count ? s.total / s.count : 0;
}

int main(void) {
    int xs[] = {3, 4, 5};
    return mean(xs, 3);
}
