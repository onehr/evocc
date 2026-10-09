/* Written for this corpus.
 * Summarize a block of signed samples with a bounded integer transform.
 * Increment saturates at 2147483647. All other inputs increase by one.
 * The returned count is independent of the checksum used by library callers.
 */
typedef struct
{
    unsigned long long checksum;
    unsigned count;
} Summary;

static unsigned next_value(unsigned state)
{
    return state * 1664525u + 1013904223u;
}

static void make_samples(int *samples, unsigned seed, unsigned count)
{
    unsigned position;
    unsigned state = seed;
    for (position = 0; position < count; ++position)
    {
        state = next_value(state);
        samples[position] = (int)(state & 1023u) - 512;
    }
    if (count != 0)
    {
        samples[0] = 2147483647;
    }
}

static unsigned parameter_at(unsigned position, unsigned seed)
{
    return (position + seed) % 5u;
}

static int transform(int value)
{
    return value == 2147483647 ? 2147483647 : value + 1;
}

static unsigned long long mix(unsigned long long hash, unsigned value)
{
    hash ^= value;
    hash *= 1099511628211ull;
    return hash;
}

static Summary summarize(const int *samples, unsigned count, unsigned seed)
{
    Summary result;
    unsigned position;
    result.checksum = 1469598103934665603ull + seed;
    result.count = 0;
    for (position = 0; position < count; ++position)
    {
        unsigned parameter = parameter_at(position, seed);
        int value = transform(samples[position]);
        result.checksum = mix(result.checksum, (unsigned)value);
        result.checksum = mix(result.checksum, parameter);
        ++result.count;
    }
    return result;
}

static Summary process(unsigned seed)
{
    int samples[32];
    make_samples(samples, seed, 32);
    return summarize(samples, 32, seed);
}

unsigned long long evaluate(unsigned seed)
{
    Summary result = process(seed);
    return result.checksum;
}

int main(void)
{
    Summary result = process(5);
    return (int)result.count;
}
