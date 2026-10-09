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
    return view->owned[index];
}

/* Adler-32 uses two sums modulo 65521, as in zlib 1.3.2. */
static unsigned long long checksum(View *view, unsigned seed)
{
    unsigned first = (seed & 65535u);
    unsigned second = (seed >> 16);
    unsigned remaining = view->length;
    unsigned position = 0;
    while (remaining != 0)
    {
        unsigned block = remaining < 5552u ? remaining : 5552u;
        remaining -= block;
        while (block != 0)
        {
            first += byte_at(view, position);
            second += first;
            ++position;
            --block;
        }
        first %= 65521u;
        second %= 65521u;
    }
    return first | (second << 16);
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
