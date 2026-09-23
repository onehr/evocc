/* The integer minimum shape in the unchanged Floyd-Warshall inner loop. */
int minimum(const int *a, const int *b) { return *a < *b ? *a : *b; }
int maximum(const int *a, const int *b) { return *a > *b ? *a : *b; }
int reversed(const int *a, const int *b) { return *a <= *b ? *b : *a; }
int unequal(const int *a, const int *b) { return *a != *b ? *a : *b; }
int equal_reversed(const int *a, const int *b) { return *a == *b ? *b : *a; }
int greater_equal(const int *a, const int *b) { return *a >= *b ? *a : *b; }
unsigned common(int a, unsigned b) { return a < b ? a : b; }
int narrow(unsigned char a, unsigned char b) { return a < b ? a : b; }
int row_minimum(const int (*p)[3], int i, int j)
{
    return p[i][j] < p[i][0] + p[0][j]
        ? p[i][j] : p[i][0] + p[0][j];
}

/* These look similar but must keep the second evaluation. */
int volatile_minimum(volatile int *a, volatile int *b)
{
    return *a < *b ? *a : *b;
}
int other_object(const int *a, const int *b, const int *c)
{
    return *a < *b ? *c : *b;
}
int tick(int *p) { return (*p)++; }
int effectful(int *p) { return tick(p) < 10 ? tick(p) : 10; }
int indexed_effect(int *p, int *count)
{
    return p[(*count)++] < 10 ? p[(*count)++] : 10;
}
int atomic_minimum(int *p)
{
    return __atomic_load_n(p, 0) < 10 ? __atomic_load_n(p, 0) : 10;
}
struct pair { int a, b; };
int member_minimum(const struct pair *p)
{
    return p->a < p->b ? p->a : p->b;
}
__int128 wide_minimum(__int128 a, __int128 b) { return a < b ? a : b; }
int retained(int a, int b) { return 100 + (a < b ? a : b); }

int main(void)
{
    int values[] = {-8, 3, 0, 3, 20};
    for (unsigned i = 0; i < 5; ++i) {
        for (unsigned j = 0; j < 5; ++j) {
            int a = values[i], b = values[j];
            int low = a < b ? a : b, high = a > b ? a : b;
            if (minimum(&a, &b) != low || maximum(&a, &b) != high
                || reversed(&a, &b) != high || unequal(&a, &b) != a
                || equal_reversed(&a, &b) != a || greater_equal(&a, &b) != high)
                return 1;
        }
    }
    const int rows[2][3] = {{4, 6, 9}, {3, 12, 2}};
    if (row_minimum(rows, 1, 1) != 9 || row_minimum(rows, 1, 2) != 2)
        return 2;
    if (common(-1, 2) != 2 || common(3, 7) != 3 || narrow(250, 7) != 7)
        return 3;
    volatile int a = 3, b = 7;
    if (volatile_minimum(&a, &b) != 3)
        return 4;
    int c = 99, d = 2, e = 7;
    if (other_object(&d, &e, &c) != 99)
        return 5;
    int counter = 5;
    if (effectful(&counter) != 6 || counter != 7)
        return 6;
    int index = 0;
    if (indexed_effect(values + 1, &index) != 0 || index != 2)
        return 7;
    if (atomic_minimum(&counter) != 7)
        return 8;
    struct pair pair = {8, -2};
    if (member_minimum(&pair) != -2 || retained(3, 8) != 103
        || retained(8, 3) != 103)
        return 9;
    __int128 wide = (__int128)1 << 100;
    if (wide_minimum(wide, wide + 1) != wide
        || wide_minimum(-wide, wide) != -wide)
        return 10;
    return 0;
}
