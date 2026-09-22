/* Admission may be reused only while every incoming path retains FPCR. */
extern void foreign(void);

double chain(double a, double b) { return (a + b) * a / b; }
double loop(int n, double a)
{
    double value = a + 1.0;
    for (int i = 0; i < n; ++i)
        value = value * a + 1.0;
    return value;
}
double both(int take, double a)
{
    if (take) a += 1.0;
    else a -= 1.0;
    return a * 2.0;
}
double one(int take, double a)
{
    if (take) a += 1.0;
    return a * 2.0;
}
double after_call(double a)
{
    double before = a + 1.0;
    foreign();
    return before * 2.0;
}
double after_asm(double a)
{
    double before = a + 1.0;
    __asm__ volatile ("" ::: "memory");
    return before * 2.0;
}
double after_asm_goto(double a)
{
    double before = a + 1.0;
    __asm__ goto ("" : : : "memory" : changed);
changed:
    return before * 2.0;
}
double transport(double a) { return a; }
