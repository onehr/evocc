/* Exact rational resampling of a finite integer signal.
 * Positions advance by numerator / denominator, with both in 1..31.
 * Samples are in -100000000..100000000. Linear interpolation rounds
 * toward negative infinity. The last sample extends to the final position.
 */
typedef struct {
    int samples[24];
    unsigned count;
} Signal;

typedef struct {
    unsigned index;
    unsigned fraction;
    unsigned numerator;
    unsigned denominator;
} Cursor;

static unsigned gcd(unsigned a, unsigned b)
{
    while (b != 0) {
        unsigned remainder = a % b;
        a = b;
        b = remainder;
    }
    return a;
}

static int cursor_initialize(Cursor *cursor, unsigned numerator, unsigned denominator)
{
    unsigned common;
    if (numerator == 0 || numerator > 31 || denominator == 0 || denominator > 31)
        return 0;
    common = gcd(numerator, denominator);
    cursor->index = 0;
    cursor->fraction = 0;
    cursor->numerator = numerator / common;
    cursor->denominator = denominator / common;
    return 1;
}

static void advance(Cursor *cursor)
{
    cursor->fraction += cursor->numerator;
    cursor->index += cursor->fraction / cursor->denominator;
    cursor->fraction %= cursor->denominator;
}

static long long floor_divide(long long numerator, unsigned denominator)
{
    long long quotient = numerator / denominator;
    long long remainder = numerator % denominator;
    if (remainder > 0)
        --quotient;
    return quotient;
}

static int interpolate(int left, int right, unsigned fraction, unsigned denominator)
{
    int difference = right - left;
    long long scaled = difference * (int)fraction;
    return left + (int)floor_divide(scaled, denominator);
}

static unsigned resample(const Signal *source, unsigned numerator, unsigned denominator,
                         int *destination, unsigned capacity)
{
    Cursor cursor;
    unsigned count = 0;
    if (source->count == 0 || !cursor_initialize(&cursor, numerator, denominator))
        return 0;
    while (cursor.index < source->count && count < capacity) {
        unsigned next = cursor.index + 1;
        int value;
        if (next == source->count)
            next = cursor.index;
        value = interpolate(source->samples[cursor.index], source->samples[next],
                            cursor.fraction, cursor.denominator);
        destination[count++] = value;
        advance(&cursor);
    }
    return count;
}

static void signal_initialize(Signal *signal, unsigned seed)
{
    unsigned i;
    signal->count = 10 + seed % 4;
    for (i = 0; i < signal->count; ++i) {
        unsigned value = (i * 313U + seed * 193U + i * i * 7U) % 2001U;
        signal->samples[i] = (int)value - 1000;
    }
    if (seed == 11) {
        signal->samples[7] = -100000000;
        signal->samples[8] = 100000000;
    }
}

static void reverse(Signal *signal)
{
    unsigned i;
    for (i = 0; i < signal->count / 2; ++i) {
        unsigned opposite = signal->count - 1 - i;
        int value = signal->samples[i];
        signal->samples[i] = signal->samples[opposite];
        signal->samples[opposite] = value;
    }
}

static unsigned long long summarize(const int *samples, unsigned count)
{
    long long sum = 0;
    long long variation = 0;
    unsigned long long hash = count;
    unsigned i;
    for (i = 0; i < count; ++i) {
        sum += samples[i];
        if (i > 0) {
            long long difference = (long long)samples[i] - samples[i - 1];
            variation += difference < 0 ? -difference : difference;
        }
        hash = hash * 65599U + (unsigned)samples[i];
    }
    return hash ^ (unsigned long long)sum ^ (unsigned long long)variation;
}

/* The stream generator accepts all unsigned seeds, modulo twelve. */
unsigned long long evaluate(unsigned seed)
{
    Signal source;
    int output[512];
    unsigned count;
    unsigned long long result;
    seed %= 12;
    signal_initialize(&source, seed);
    count = resample(&source, 3, 17, output, 512);
    result = summarize(output, count);
    count = resample(&source, 3 + seed % 5, 7 + seed % 11, output, 512);
    result = result * 37U + summarize(output, count);
    reverse(&source);
    count = resample(&source, 5, 13, output, 512);
    result = result * 37U + summarize(output, count);
    count = resample(&source, 3, 1, output, 512);
    return result ^ summarize(output, count);
}

int main(void)
{
    unsigned long long checksum = evaluate(0);
    checksum ^= evaluate(1);
    checksum += evaluate(2);
    checksum ^= evaluate(11);
    return checksum == 0xffffffffffffffffULL;
}
