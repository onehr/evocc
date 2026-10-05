/* The companion IR retains the private object across a checked arithmetic
 * edge. This source supplies its signature and intended scalar behavior; the
 * IR is a deliberate storage fixture, not a claimed frontend export. */
int private_checked(int value)
{
    int saved = value;
    saved += 1;
    return saved;
}
