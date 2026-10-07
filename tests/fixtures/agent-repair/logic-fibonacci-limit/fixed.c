/* Adapted from TinyCC tests under GNU LGPL v2.1; see ../LICENSE.tinycc.
 * Modified for the EvoCC agent-repair corpus on 2026-10-07.
 */

int fibonacci_ceiling(int limit);
int main(void);

/* Return the first Fibonacci number at least limit for limit in [1, 1000]; otherwise return 0. */
int fibonacci_ceiling(int limit)
{
    int a = 1;
    int p = 0;
    int t;
    if (limit < 1 || limit > 1000)
        return 0;
    while (a < limit) {
        t = a;
        a = t + p;
        p = t;
    }
    return a;
}

int main(void)
{
    return fibonacci_ceiling(10) != 13;
}
