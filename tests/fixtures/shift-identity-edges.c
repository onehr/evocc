/* Reduction of LLVM shiftopt-1: preserve evaluations while pruning dead edges. */
extern void impossible_shift(void);
void unsigned_shifts(unsigned x) {
    if ((x >> 0) != x || (x << 0) != x) impossible_shift();
    if ((0 << x) != 0 || (0 >> x) != 0) impossible_shift();
    if ((-1 >> x) != -1 || (~0 >> x) != ~0) impossible_shift();
}
void signed_shifts(int x) {
    if ((x >> 0) != x || (x << 0) != x) impossible_shift();
    if ((0 << x) != 0 || (0 >> x) != 0) impossible_shift();
}
int main(void) {
    int effects = 0;
    unsigned_shifts(0); unsigned_shifts(9);
    signed_shifts(0); signed_shifts(9);
    if ((0 << effects++) != 0 || effects != 1) return 1;
    return 0;
}
