unsigned long nine(unsigned long, unsigned long, unsigned long, unsigned long,
                   unsigned long, unsigned long, unsigned long, unsigned long,
                   unsigned long);
long packed(long, long, long, long, long, long, long, long,
            signed char, short, int, ...);

int main(void)
{
    if (nine(1, 2, 3, 4, 5, 6, 7, 8, 9) != 45)
        return 1;
    if (packed(1, 2, 3, 4, 5, 6, 7, 8, -3, -500, 700, 900L) != 1133)
        return 2;
    return 0;
}
