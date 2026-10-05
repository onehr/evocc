/* The companion IR keeps private objects in both caller and callee. The
 * discarded parameter also exercises the existing zero-substitution pass.
 * This is a deliberate storage fixture, not a claimed frontend export. */
static unsigned int private_keep(unsigned int value, unsigned int discarded)
{
    unsigned int saved = value;
    return saved;
}

unsigned int private_call_loop(unsigned int count, unsigned int value)
{
    unsigned int saved = value;
    while (count != 0) {
        saved = private_keep(saved, count) + 1;
        --count;
    }
    return saved;
}
