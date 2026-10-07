/* Adapted from TinyCC tests under GNU LGPL v2.1; see ../LICENSE.tinycc.
 * Modified for the EvoCC agent-repair corpus on 2026-10-07.
 */

int switch_trace(int value);
int main(void);

/* Encode the switch trace as decimal digits: cases 1 and 3 return immediately as 1 or 3; case 2 emits 2 then the common exit digit 9, yielding 29; all other inputs yield 9. */
int switch_trace(int value)
{
    int trace = 0;
    switch (value) {
    case 1:
        return 1;
    case 2:
        trace = 2;
        break;
    case 3:
        return 3;
    default:
        break;
    }
    return trace * 10 + 9;
}

int main(void)
{
    return switch_trace(2) != 29;
}
