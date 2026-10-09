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
    view->data = input;
    view->length = 16;
    view->seed += seed;
}

static unsigned char byte_at(const View *view, unsigned index)
{
    return view->data[index];
}

/* JSON pointer escaping from cJSON 1.7.19, with an explicit byte length. */
static unsigned long long checksum(View *view, unsigned seed)
{
    unsigned char encoded[32];
    unsigned length = 0;
    unsigned position = 0;
    unsigned long long hash = seed;
    while (position < view->length)
    {
        unsigned char value = byte_at(view, position);
        if (value == '/' || value == '~')
        {
            encoded[length++] = '~';
            encoded[length++] = value == '/' ? '1' : '0';
        }
        else
        {
            encoded[length++] = value;
        }
        ++position;
    }
    for (position = 0; position < length; ++position)
    {
        hash = hash * 257u + encoded[position];
    }
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
