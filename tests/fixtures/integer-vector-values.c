typedef int i4 __attribute__((vector_size(16)));
typedef unsigned int u4 __attribute__((vector_size(16)));
typedef unsigned char u16 __attribute__((vector_size(16)));
typedef short s4 __attribute__((vector_size(8)));
struct holder { char prefix; i4 lanes; };
_Static_assert(sizeof(i4) == 16, "vector size");
_Static_assert(_Alignof(i4) == 16, "vector alignment");
_Static_assert(sizeof(s4) == 8, "short vector");
_Static_assert(sizeof(struct holder) == 32, "member layout");
static i4 stored = {1, 2, 3, 4};
static i4 add(i4 a, i4 b) { return a + b; }
int main(void) {
    i4 a = stored, b = {4, 3, 2, 1};
    i4 sum = add(a, b);
    if (sum[0] != 5 || sum[1] != 5 || sum[2] != 5 || sum[3] != 5) return 1;
    i4 mask = a < b;
    if (mask[0] != -1 || mask[1] != -1 || mask[2] != 0 || mask[3] != 0) return 2;
    a[1] += 7;
    if (a[1] != 9 || stored[1] != 2) return 3;
    u4 x = {0xffffffffu, 2, 3, 4};
    x = (x + 1u) * 2u;
    if (x[0] != 0 || x[1] != 6 || x[3] != 10) return 4;
    u16 small = {255, 1};
    small = small + 1;
    if (sizeof(small + small) != 16 || small[0] != 0 || small[1] != 2) return 5;
    i4 indices = {3, 0, 5, 6};
    i4 shuffled = __builtin_shuffle(stored, b, indices);
    if (shuffled[0] != 4 || shuffled[1] != 1 || shuffled[2] != 3 || shuffled[3] != 2) return 6;
    i4 shifted = (i4){-1, 1, 2, 3} << (i4){1, 31, 2, 0};
    if (shifted[0] != -2 || shifted[1] != (-2147483647 - 1) || shifted[2] != 8) return 7;
    return 0;
}
