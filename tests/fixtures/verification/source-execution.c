/* Whole-tree execution correspondence; no verified-compilation claim. */
#include "doom-fixedmul.c"
#include "musl-abs.c"
#include "control.c"
#include "pressure.c"

static int frame(int x)
{
    x += 1;
    return x;
}

static int caller(int x)
{
    int y = frame(x);
    {
        int z = 8;
        y += z;
        x += 2;
    }
    return x + y;
}

static int control(int n)
{
    int count = 0;
    for (int i = 0; i < n; ++i) {
        if (i == 2)
            continue;
        if (i == 5)
            break;
        ++count;
    }
    do {
        --n;
    } while (n > 10);
    return count + n;
}

int main(void)
{
    int x = 4;
    int before = x++;
    return before != 4 || x != 5 || caller(5) != 21 || control(6) != 9 ||
           FixedMul(98304, 131072) != 196608 ||
           FixedMul(-98304, 131072) != -196608 ||
           abs(-9) != 9 || abs(0) != 0 || abs(7) != 7 ||
           loop_join(1U, 4U) != 0xc000000dU ||
           loop_join(1U, 64U) != 0xffe0007aU ||
           short_circuit(0) != 2 || short_circuit(1) != 11 ||
           pressure(17UL) != 1040UL ||
           (unsigned long)-1 != 0xffffffffffffffffUL ||
           (long)0xffffffffU != 4294967295L ||
           -7 / 3 != -2 || -7 % 3 != -1 ||
           (unsigned)(2147483647 + 1) != 0x80000000U;
}
