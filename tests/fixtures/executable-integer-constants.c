volatile int external_value = 7;
int calls;
int next(void) { ++calls; return external_value; }
unsigned long word(void) { return 13UL / (sizeof(unsigned long) * 8); }

int main(void)
{
    if (word() || 130UL / (sizeof(unsigned long) * 8) != 2)
        return 1;
    if ((-1 << 1) != -2 || (1U << 31) != 0x80000000U)
        return 2;
    if ((((unsigned __int128)1 << 100) >> 96) != 16)
        return 3;
    if (next() / (sizeof(unsigned long) * 8) != 0 || calls != 1)
        return 4;
    int n = 3;
    unsigned long size = sizeof(int[n++]) + 5;
    if (size != 17 || n != 4)
        return 5;
    if ((1 || next()) != 1 || (0 && next()) != 0 || calls != 1)
        return 6;
    if ((external_value + 2) * 3 != 27)
        return 7;
    return 0;
}
