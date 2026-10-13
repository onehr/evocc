int divide(int divisor, int unused)
{
    if (divisor < 0)
        return 0;
    if (divisor == 0) return 0;
    return 12 / divisor;
}
int main(void) { return divide(0, 7); }
