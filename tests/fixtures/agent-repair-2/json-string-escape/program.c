/* Altered cJSON source; MIT notice retained in ../LICENSE.cjson. */
/* Null text prints an empty JSON string. The bounded inline buffer cannot
 * grow; insufficient capacity returns zero.
 * Only evaluate seeds 0..11 are supported.
 */
typedef struct
{
    unsigned char data[128];
    unsigned limit;
    unsigned length;
} PrintBuffer;
static unsigned char *ensure(PrintBuffer *buffer, unsigned required)
{
    if (buffer == 0 || required > buffer->limit || required > 128)
    {
        return 0;
    }
    return buffer->data;
}
static unsigned text_length(const unsigned char *text)
{
    unsigned n = 0;
    if (text == 0)
    {
        return 0;
    }
    while (text[n] != 0)
    {
        n++;
    }
    return n;
}
static int print_string(const unsigned char *input, PrintBuffer *buffer)
{
    const unsigned char *input_pointer = input;
    unsigned char *output = 0;
    unsigned char *output_pointer = 0;
    unsigned output_length = 0;
    unsigned escape_characters = 0;
    const unsigned char digits[17] = "0123456789abcdef";
    if (buffer == 0)
    {
        return 0;
    }
    buffer->length = 0;
    for (input_pointer = input; *input_pointer; input_pointer++)
    {
        switch (*input_pointer)
        {
            case '"':
            case '\\':
            case '\b':
            case '\f':
            case '\n':
            case '\r':
            case '\t':
                escape_characters++;
                break;
            default:
                if (*input_pointer < 32)
                {
                    escape_characters += 5;
                }
                break;
        }
    }
    output_length = (unsigned)(input_pointer - input) + escape_characters;
    output = ensure(buffer, output_length + 3);
    if (output == 0)
    {
        return 0;
    }
    if (escape_characters == 0)
    {
        unsigned i = 0;
        output[0] = '"';
        for (i = 0; i < output_length; i++)
        {
            output[i + 1] = input[i];
        }
        output[output_length + 1] = '"';
        output[output_length + 2] = 0;
        buffer->length = output_length + 2;
        return 1;
    }
    output[0] = '"';
    output_pointer = output + 1;
    for (input_pointer = input; *input_pointer != 0; input_pointer++, output_pointer++)
    {
        if (*input_pointer > 31 && *input_pointer != '"' && *input_pointer != '\\')
        {
            *output_pointer = *input_pointer;
        }
        else
        {
            *output_pointer++ = '\\';
            switch (*input_pointer)
            {
                case '\\':
                    *output_pointer = '\\';
                    break;
                case '"':
                    *output_pointer = '"';
                    break;
                case '\b':
                    *output_pointer = 'b';
                    break;
                case '\f':
                    *output_pointer = 'f';
                    break;
                case '\n':
                    *output_pointer = 'n';
                    break;
                case '\r':
                    *output_pointer = 'r';
                    break;
                case '\t':
                    *output_pointer = 't';
                    break;
                default:
                    output_pointer[0] = 'u';
                    output_pointer[1] = '0';
                    output_pointer[2] = '0';
                    output_pointer[3] = digits[(*input_pointer >> 4) & 15u];
                    output_pointer[4] = digits[(*input_pointer >> 4) & 15u];
                    output_pointer += 4;
                    break;
            }
        }
    }
    output[output_length + 1] = '"';
    output[output_length + 2] = 0;
    buffer->length = output_length + 2;
    return 1;
}
static unsigned hash_printed(const PrintBuffer *buffer)
{
    unsigned hash = buffer->length;
    unsigned i = 0;
    for (i = 0; i < buffer->length; i++)
    {
        hash = (hash * 33u + buffer->data[i]) & 65535u;
    }
    return hash;
}
static unsigned count_escapes(const PrintBuffer *buffer)
{
    unsigned count = 0;
    unsigned i = 0;
    for (i = 0; i < buffer->length; i++)
    {
        if (buffer->data[i] == '\\')
        {
            count++;
            if (i + 1 < buffer->length)
            {
                i++;
            }
        }
    }
    return count;
}
unsigned evaluate(unsigned seed)
{
    unsigned char input[12];
    PrintBuffer full;
    PrintBuffer short_buffer;
    unsigned i = 0;
    unsigned hash = 0;
    for (i = 0; i < 12; i++)
    {
        input[i] = 0;
    }
    input[0] = (unsigned char)('a' + seed % 20u);
    input[1] = '\n';
    input[2] = '"';
    input[3] = (unsigned char)(1u + seed % 7u);
    input[4] = '\\';
    input[5] = (unsigned char)('0' + seed % 10u);
    full.limit = 128;
    full.length = 0;
    short_buffer.limit = 8u + seed % 3u;
    short_buffer.length = 0;
    if (print_string(input, &full))
    {
        hash = hash_printed(&full) + 19u * count_escapes(&full);
    }
    hash += (unsigned)print_string(input, &short_buffer) * 7u;
    hash += text_length(input);
    if (print_string(seed == 11u ? 0 : (const unsigned char *)"key", &full))
    {
        hash = (hash * 31u + hash_printed(&full)) & 65535u;
    }
    return hash & 65535u;
}
int main(void)
{
    unsigned result = evaluate(0);
    result ^= evaluate(1);
    result ^= evaluate(2);
    result ^= evaluate(11);
    return (int)(result & 255u);
}
