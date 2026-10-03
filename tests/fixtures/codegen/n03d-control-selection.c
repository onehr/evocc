extern unsigned inspect(unsigned);

unsigned late_branch(unsigned x) {
    if (inspect(x)) return 7;
    unsigned local = x + 3U;
    return local;
}

unsigned interval_signed(int x) {
    switch (x) {
    case 48 ... 57: return 1;
    default: return 0;
    }
}

unsigned interval_unsigned(unsigned long x) {
    switch (x) {
    case 102 ... 107: return 1;
    default: return 0;
    }
}
