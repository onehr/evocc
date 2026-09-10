typedef unsigned __int128 U;
typedef __int128 I;

__attribute__((noinline)) U unsigned_quotient(U a, U b) { return a / b; }
__attribute__((noinline)) U unsigned_remainder(U a, U b) { return a % b; }
__attribute__((noinline)) I signed_quotient(I a, I b) { return a / b; }
__attribute__((noinline)) I signed_remainder(I a, I b) { return a % b; }

/* Each pair remains live across other helper calls, including stack arguments. */
__attribute__((noinline)) U live_pairs(U a, U b, U c, U d, U e, U f) {
    U q = a / b;
    U r = c % d;
    U t = e / f;
    return q + r + t + a + b + c + d + e + f;
}

int main(void) {
    U n = ((U)1 << 100) + 7;
    U q = ((U)0x555555555UL << 64) | 0x5555555555555557UL;
    if (unsigned_quotient(n, 3) != q || unsigned_remainder(n, 3) != 2) return 1;
    U d = ((U)1 << 80) + 3;
    if (unsigned_quotient(d * 17 + 5, d) != 17) return 2;
    if (unsigned_remainder(d * 17 + 5, d) != 5) return 3;
    U maximum = ~(U)0;
    if (unsigned_quotient(maximum, 2) != (maximum >> 1)) return 4;
    if (unsigned_remainder(maximum, 2) != 1) return 5;
    if (signed_quotient(-(I)n, 3) != -(I)q) return 6;
    if (signed_remainder(-(I)n, 3) != -2) return 7;
    if (signed_quotient(-(I)n, -3) != (I)q) return 8;
    if (signed_remainder((I)n, -3) != 2) return 9;
    I minimum = -((I)1 << 126) * 2;
    if (signed_quotient(minimum, 1) != minimum) return 10;
    if (signed_quotient(minimum, minimum) != 1) return 11;
    if (signed_remainder(minimum, minimum) != 0) return 12;
    volatile U stored = n;
    stored /= 3;
    if (stored != q) return 13;
    U values[2] = {n, n};
    int visits = 0;
    values[visits++] %= 3;
    if (visits != 1 || values[0] != 2 || values[1] != n) return 14;
    if (live_pairs(n, 3, d * 17 + 5, d, maximum, 2)
        != q + 5 + (maximum >> 1) + n + 3 + (d * 17 + 5) + d + maximum + 2)
        return 15;
    return 0;
}
