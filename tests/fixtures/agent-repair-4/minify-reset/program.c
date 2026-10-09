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

/* Comment and whitespace removal from cJSON 1.7.19. */
static void skip_line(char **input)
{
    *input += 2;
    while (**input != 0)
    {
        if (*(*input)++ == '\n')
            return;
    }
}

static void skip_block(char **input)
{
    *input += 2;
    while (**input != 0)
    {
        if ((*input)[0] == '*' && (*input)[1] == '/')
        {
            *input += 2;
            return;
        }
        ++*input;
    }
}

static void copy_string(char **input, char **output)
{
    *(*output)++ = *(*input)++;
    while (**input != 0)
    {
        *(*output)++ = **input;
        if (*(*input)++ == '"')
            return;
        if ((*input)[-1] == '\\' && **input == '"')
            *(*output)++ = *(*input)++;
    }
}

static unsigned long long checksum(View *view, unsigned seed)
{
    char text[17];
    char *input = text;
    char *output = text;
    unsigned position;
    unsigned long long hash = seed;
    for (position = 0; position < view->length; ++position)
        text[position] = (char)byte_at(view, position);
    text[view->length] = 0;
    while (*input != 0)
    {
        switch (*input)
        {
            case ' ': case '\t': case '\r': case '\n':
                ++input;
                break;
            case '/':
                if (input[1] == '/') skip_line(&input);
                else if (input[1] == '*') skip_block(&input);
                else ++input;
                break;
            case '"':
                copy_string(&input, &output);
                break;
            default:
                *output++ = *input++;
                break;
        }
    }
    *output = 0;
    for (input = text; *input != 0; ++input)
        hash = hash * 257u + (unsigned char)*input;
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
