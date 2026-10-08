/* A bounded streaming window with exact integer moments.
 * Capacity is 1..16. Samples are in -100000..100000.
 * Resize keeps the newest samples in chronological order.
 * The variance numerator is count * sum_of_squares - sum * sum.
 */
typedef struct {
    int samples[16];
    unsigned capacity;
    unsigned count;
    unsigned next;
    long long sum;
    long long squares;
} Window;

static long long square(int sample)
{
    return (long long)sample * sample;
}

static int initialize(Window *window, unsigned capacity)
{
    unsigned i;
    if (capacity == 0 || capacity > 16)
        return 0;
    window->capacity = capacity;
    window->count = 0;
    window->next = 0;
    window->sum = 0;
    window->squares = 0;
    for (i = 0; i < 16; ++i)
        window->samples[i] = 0;
    return 1;
}

static unsigned oldest(const Window *window)
{
    if (window->count == window->capacity)
        return window->next;
    return 0;
}

static int at(const Window *window, unsigned index)
{
    unsigned position = oldest(window) + index;
    if (position >= window->capacity)
        position -= window->capacity;
    return window->samples[position];
}

static void push(Window *window, int sample)
{
    if (window->count == window->capacity) {
        unsigned removed = window->next;
        window->sum -= window->samples[removed];
        window->squares -= square(window->samples[removed]);
    } else {
        ++window->count;
    }
    window->samples[window->next] = sample;
    window->sum += sample;
    window->squares += square(sample);
    ++window->next;
    if (window->next == window->capacity)
        window->next = 0;
}

static void resize(Window *window, unsigned capacity)
{
    int retained[16];
    unsigned keep = window->count;
    unsigned start;
    unsigned i;
    if (capacity == 0 || capacity > 16)
        return;
    if (keep > capacity)
        keep = capacity;
    start = window->count - keep;
    for (i = 0; i < keep; ++i)
        retained[i] = at(window, start + i);
    initialize(window, capacity);
    for (i = 0; i < keep; ++i)
        push(window, retained[i]);
}

static void range(const Window *window, int *minimum, int *maximum)
{
    unsigned i;
    *minimum = 0;
    *maximum = 0;
    if (window->count == 0)
        return;
    *minimum = at(window, 0);
    *maximum = *minimum;
    for (i = 1; i < window->count; ++i) {
        int value = at(window, i);
        if (value < *minimum)
            *minimum = value;
        if (value > *maximum)
            *maximum = value;
    }
}

static long long variance_numerator(const Window *window)
{
    return window->count * window->squares - window->sum * window->sum;
}

static unsigned long long fingerprint(const Window *window)
{
    unsigned long long hash = 1469598103934665603ULL;
    unsigned i;
    int minimum;
    int maximum;
    range(window, &minimum, &maximum);
    for (i = 0; i < window->count; ++i) {
        hash ^= (unsigned)at(window, i);
        hash *= 1099511628211ULL;
    }
    hash ^= (unsigned long long)window->sum;
    hash *= 1099511628211ULL;
    hash ^= (unsigned long long)variance_numerator(window);
    hash *= 1099511628211ULL;
    hash ^= (unsigned)minimum;
    hash = hash * 31U + (unsigned)maximum;
    return hash;
}

static int sample_for(unsigned seed, unsigned index)
{
    unsigned value = (seed * 73U + index * 97U + index * index * 13U) % 2001U;
    int sample = (int)value - 1000;
    if (seed == 11 && index == 19)
        sample = 50000;
    return sample;
}

/* Every seed is valid; the deterministic stream family repeats every 12. */
unsigned long long evaluate(unsigned seed)
{
    Window window;
    unsigned i;
    unsigned capacity;
    unsigned long long result = 0;
    seed %= 12;
    capacity = 3 + seed % 11;
    initialize(&window, capacity);
    for (i = 0; i < 32; ++i) {
        push(&window, sample_for(seed, i));
        if (i == 8)
            resize(&window, 2 + seed % 5);
        if (i == 17)
            resize(&window, 8 + seed % 8);
        if ((i & 3U) == 3U)
            result = result * 37U + fingerprint(&window);
    }
    resize(&window, 1);
    result ^= fingerprint(&window);
    resize(&window, 16);
    for (i = 32; i < 40; ++i)
        push(&window, sample_for(seed, i));
    return result ^ fingerprint(&window);
}

int main(void)
{
    unsigned long long checksum = evaluate(0);
    checksum ^= evaluate(1);
    checksum += evaluate(2);
    checksum ^= evaluate(11);
    return checksum == 0xffffffffffffffffULL;
}
