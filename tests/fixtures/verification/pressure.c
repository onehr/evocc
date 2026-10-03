unsigned long wide(
    unsigned long a00, unsigned long a01, unsigned long a02, unsigned long a03,
    unsigned long a04, unsigned long a05, unsigned long a06, unsigned long a07,
    unsigned long a08, unsigned long a09, unsigned long a10, unsigned long a11,
    unsigned long a12, unsigned long a13, unsigned long a14, unsigned long a15,
    unsigned long a16, unsigned long a17, unsigned long a18, unsigned long a19,
    unsigned long a20, unsigned long a21, unsigned long a22, unsigned long a23,
    unsigned long a24, unsigned long a25, unsigned long a26, unsigned long a27,
    unsigned long a28, unsigned long a29, unsigned long a30, unsigned long a31)
{
    return a00 + a01 + a02 + a03 + a04 + a05 + a06 + a07
         + a08 + a09 + a10 + a11 + a12 + a13 + a14 + a15
         + a16 + a17 + a18 + a19 + a20 + a21 + a22 + a23
         + a24 + a25 + a26 + a27 + a28 + a29 + a30 + a31;
}

unsigned long pressure(unsigned long x)
{
    return wide(x + 0UL, x + 1UL, x + 2UL, x + 3UL,
                x + 4UL, x + 5UL, x + 6UL, x + 7UL,
                x + 8UL, x + 9UL, x + 10UL, x + 11UL,
                x + 12UL, x + 13UL, x + 14UL, x + 15UL,
                x + 16UL, x + 17UL, x + 18UL, x + 19UL,
                x + 20UL, x + 21UL, x + 22UL, x + 23UL,
                x + 24UL, x + 25UL, x + 26UL, x + 27UL,
                x + 28UL, x + 29UL, x + 30UL, x + 31UL);
}
