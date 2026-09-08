static int single(int x) {
    switch (x) case 3: return 7;
    return 9;
}

static int nested(int x) {
    int n = 0;
    switch (x) {
        case 0: if (++n == 2) {
            case 1: n += 3;
        }
        n += 5;
        break;
        { case 2 ... 3: n = 11; break; }
        default: n = 17;
    }
    return n;
}

static int loops(void) {
    int total = 0, guards = 0;
    for (int i = 0; i < 3; i++) {
        switch (i) {
            if (++guards) {
                case 0: total += 1; continue;
                case 1: switch (i + 1) { case 2: total += 3; break; }
                        break;
                default: total += 5;
            }
        }
        total += 10;
    }
    return total * 100 + guards;
}

static void transfer(int *out, int *in, int count) {
    int n = (count + 3) / 4;
    switch (count % 4) {
        case 0: do { *out++ = *in++;
        case 3:      *out++ = *in++;
        case 2:      *out++ = *in++;
        case 1:      *out++ = *in++;
                } while (--n > 0);
    }
}

static int promoted(unsigned char x) {
    switch (x) {
        case 256: return 1;
        case 0: return 7;
        default: return 9;
    }
}

int main(void) {
    if (single(3) != 7 || single(4) != 9) return 1;
    if (nested(0) != 6 || nested(1) != 8 || nested(2) != 11 ||
        nested(3) != 11 || nested(4) != 17) return 2;
    if (loops() != 2900) return loops();
    int in[9] = {1,2,3,4,5,6,7,8,9}, out[9];
    for (int n = 1; n <= 9; n++) {
        for (int i = 0; i < 9; i++) out[i] = 0;
        transfer(out, in, n);
        for (int i = 0; i < 9; i++)
            if (out[i] != (i < n ? in[i] : 0)) return 3;
    }
    if (promoted(0) != 7 || promoted(1) != 9) return 4;
    return 0;
}
