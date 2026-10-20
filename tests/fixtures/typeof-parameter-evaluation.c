/* Parameter declarations execute at definition entry, not at a prototype. */
int calls;
void prototype(int n, int (*p)[n], __typeof__((calls++, p)) q);

int operand(int n, int (*p)[n], __typeof__((calls++, p)) q)
{
    return sizeof(*p) != n * sizeof(int) || sizeof(*q) != sizeof(*p);
}

int type_name(int n, __typeof__(int[(calls++, n)]) *p)
{
    return sizeof(*p) != n * sizeof(int);
}

int nested(int n, int (*p)[n],
           void (*callback)(int m, int (*s)[m], __typeof__((calls++, s)) t))
{
    return sizeof(*p) != n * sizeof(int) || callback != 0;
}

int updated(int n, int (*p)[n], __typeof__((n++, p)) q)
{
    /* The earlier parameter's extent stays fixed when the operand changes n. */
    return n != sizeof(*p) / sizeof(int) + 1 || sizeof(*q) != sizeof(*p);
}

int main(void)
{
    int a[3];
    void local_prototype(int m, int (*s)[m], __typeof__((calls++, s)) t);
    if (calls != 0) return 1;
    if (operand(3, &a, &a) || calls != 1) return 2;
    if (operand(3, &a, &a) || calls != 2) return 3;
    if (type_name(3, &a) || calls != 3) return 4;
    if (nested(3, &a, 0) || calls != 3) return 5;
    for (int n = 1; n <= 5; ++n) {
        int b[n];
        if (updated(n, &b, &b)) return 6;
    }
    return 0;
}
