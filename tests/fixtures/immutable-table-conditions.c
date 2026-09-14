struct mode { int head; unsigned short total; };
static const struct mode modes[] = { { 1, 262 }, { 2, 312 } };
static __attribute__((always_inline)) inline const struct mode *pick(unsigned char n)
{
    if (n < sizeof(modes) / sizeof(modes[0]))
        return &modes[n];
    return (void *)0;
}
extern void invalid(void) __attribute__((error("immutable representation mismatch")));
int main(void)
{
    if ("message\n"[sizeof("message\n") - 2] != '\n')
        invalid();
    if (pick(0)->total != 262 || pick(1)->total != 312)
        invalid();
    return 0;
}
