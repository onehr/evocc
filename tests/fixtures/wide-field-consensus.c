struct U { unsigned long x:40; };
struct S { signed long pad:12; signed long x:52; };

int main(void) {
    struct U u = {0xffffffffffUL};
    if (!(u.x > 1) || !(u.x != 1) || !u.x) return 1;
    if ((u.x >> 8) != 0xffffffffUL) return 2;
    if (u.x * 2UL != 0x1fffffffffeUL) return 3;
    if (u.x + 1UL != 0x10000000000UL) return 4;
    if ((1 ? u.x : 0) != 0xffffffffffUL) return 5;
    if ((u.x ?: 1) != 0xffffffffffUL) return 6;
    if (+u.x != 0xffffffffffUL) return 7;
    if ((u.x += 1) != 0 || u.x != 0) return 8;
    u.x = 0xffffffffffUL;
    if (u.x++ != 0xffffffffffUL || u.x != 0) return 9;
    u.x = 0xffffffffffUL;
    if (++u.x != 0 || u.x != 0) return 10;
    if (u.x-- != 0 || u.x != 0xffffffffffUL) return 11;
    if (--u.x != 0xfffffffffeUL) return 12;
    struct S s = {0x123, 0x123456789ABCDL};
    s.x ^= 0x0008765412345678L;
    if (s.pad != 0x123 || s.x != (long)0xFFF9551175BDFDB5UL) return 13;
    s.x = 0xFFF000FFF000FL;
    s.x |= 0x0008765412345678L;
    if (s.pad != 0x123 || s.x != (long)0xFFFFFF541FFF567FUL) return 14;
    return 0;
}
