int main(void) {
    if ((1 ? -1 : 0U) < 0) return 1;
    if ((0 ? 0UL : -1L) < 0) return 2;
    if (((1 ? -1 : 0U) < 0) && 1) return 3;
    if (((1 ? -1 : 0U) < 0) || 0) return 4;
    if (((1 ? -1 : 0U) < 0) ? 1 : 0) return 5;
    if (((1 ? -1 : 0U) < 0) ?: 0) return 6;
    switch (1 ? -1 : 0U) {
        case 0xffffffffU: break;
        default: return 7;
    }
    if ((1 ? -1 : 0UL) < 0) return 8;
    if ((1 ? -1 : 0ULL) < 0) return 9;
    if (!((1 ? (signed char)-1 : (unsigned char)0) < 0)) return 10;
    if (!((0 ? 0U : -1L) < 0)) return 11;
    return 0;
}
