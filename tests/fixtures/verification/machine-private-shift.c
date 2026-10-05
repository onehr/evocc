/* The companion IR keeps a private store after the checked variable shift.
 * It is a deliberate storage fixture, not a claimed frontend export. */
unsigned int private_shift(unsigned int value, unsigned int count)
{
    unsigned int saved = value;
    saved <<= count;
    return saved;
}
