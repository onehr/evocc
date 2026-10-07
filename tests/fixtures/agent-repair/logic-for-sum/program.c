/* Adapted from TinyCC tests under GNU LGPL v2.1; see ../LICENSE.tinycc.
 * Modified for the EvoCC agent-repair corpus on 2026-10-07.
 */

int series_sum(int limit);
int main(void);

/* Return 1 + ... + limit for limit in [0, 100]; return 0 outside that range. */
int series_sum(int limit)
{
    int count;
    int total = 0;
    if (limit < 0 || limit > 100)
        return 0;
    for (count = 1; count < limit; count++)
        total += count;
    return total;
}

int main(void)
{
    return series_sum(4) != 10;
}
