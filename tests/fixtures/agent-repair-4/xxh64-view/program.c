/* Process a 16-byte block. Return its checksum and the number of bytes used.
 * Reset must accept an uninitialized automatic View. Byte access starts at
 * index zero. A saved view must remain usable after prepare returns.
 */
typedef struct
{
    const unsigned char *data;
    unsigned char owned[16];
    unsigned length;
    unsigned seed;
} View;

typedef struct
{
    unsigned long long checksum;
    unsigned count;
} Summary;

static unsigned char pattern(unsigned index, unsigned seed)
{
    unsigned selector = (index + seed) % 7u;
    if (selector == 0) return '/';
    if (selector == 1) return '~';
    return (unsigned char)('a' + (index * 5u + seed) % 26u);
}

static void prepare(View *view, const unsigned char *input, unsigned seed)
{
    unsigned char temporary[16];
    unsigned index;
    for (index = 0; index < 16; ++index)
    {
        temporary[index] = input[index];
        view->owned[index] = input[index];
    }
    view->data = temporary;
    view->length = 16;
    view->seed = seed;
}

static unsigned char byte_at(const View *view, unsigned index)
{
    return view->data[index];
}

/* XXH64's byte tail and avalanche, from Zstandard 1.5.7. */
static unsigned long long rotate_left(unsigned long long value, unsigned count)
{
    return (value << count) | (value >> (64u - count));
}

static unsigned long long checksum(View *view, unsigned seed)
{
    unsigned long long hash = seed + 0x27D4EB2F165667C5ull + view->length;
    unsigned position = 0;
    while (position < view->length)
    {
        hash ^= byte_at(view, position) * 0x27D4EB2F165667C5ull;
        hash = rotate_left(hash, 11) * 0x9E3779B185EBCA87ull;
        ++position;
    }
    hash ^= hash >> 33;
    hash *= 0xC2B2AE3D27D4EB4Full;
    hash ^= hash >> 29;
    hash *= 0x165667B19E3779F9ull;
    hash ^= hash >> 32;
    return hash;
}

static Summary process(unsigned seed)
{
    View view;
    unsigned char input[16];
    unsigned index;
    Summary result;
    for (index = 0; index < 16; ++index)
    {
        input[index] = pattern(index, seed);
    }
    prepare(&view, input, seed);
    result.checksum = checksum(&view, view.seed);
    result.count = view.length;
    return result;
}

/* Callers can use the checksum; the command reports the processed length. */
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
