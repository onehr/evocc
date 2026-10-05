/* Signatures and source behavior for machine-private.evo. The companion IR
 * keeps explicit private byte storage so the machine proof cannot rely only
 * on the scalar-SSA form emitted for these source locals. */
unsigned private_word(unsigned value)
{
    unsigned saved = value;
    return saved;
}

unsigned long long private_wide(unsigned long long value)
{
    unsigned long long saved = value;
    return saved;
}

unsigned private_caller(unsigned value)
{
    unsigned saved = value;
    return private_word(value) + saved;
}

unsigned private_loop(unsigned count, unsigned value)
{
    unsigned saved = value;
    while (count) {
        saved += 1;
        count -= 1;
    }
    return saved;
}
