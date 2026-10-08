/* Altered cJSON source; MIT notice retained in ../LICENSE.cjson. */
/* Hexadecimal fragments contain four characters; invalid or null fragments
 * return 0xffffffffU, distinct from valid U+0000. String scanning uses bounded
 * input/output buffers. Only evaluate seeds 0..11 are supported.
 */
static unsigned parse_hex4(const unsigned char *input)
{
    unsigned h = 0;
    unsigned i = 0;
    if (input == 0)
    {
        return 0xffffffffu;
    }
    for (i = 0; i < 4; i++)
    {
        if (input[i] >= '0' && input[i] <= '9')
        {
            h += (unsigned)input[i] - '0';
        }
        else if (input[i] >= 'A' && input[i] <= 'F')
        {
            h += 10u + (unsigned)input[i] - 'A';
        }
        else if (input[i] >= 'a' && input[i] <= 'f')
        {
            h += 10u + (unsigned)input[i] - 'a';
        }
        else
        {
            return 0xffffffffu;
        }
        if (i < 3)
        {
            h = h << 4;
        }
    }
    return h;
}
static unsigned decode_literal(const unsigned char *input,
                               unsigned remaining,
                               unsigned char **output)
{
    unsigned codepoint = 0;
    unsigned first_code = 0;
    unsigned utf8_length = 0;
    unsigned position = 0;
    unsigned sequence_length = 0;
    unsigned first_byte_mark = 0;
    if (input == 0 || output == 0 || remaining < 6)
    {
        return 0;
    }
    if (input[0] != '\\' || input[1] != 'u')
    {
        return 0;
    }
    first_code = parse_hex4(input + 2);
    if (first_code == 0xffffffffu ||
        (first_code >= 0xdc00u && first_code <= 0xdfffu))
    {
        return 0;
    }
    if (first_code >= 0xd800u && first_code <= 0xdbffu)
    {
        unsigned second_code = 0;
        const unsigned char *second = input + 6;
        sequence_length = 12;
        if (remaining < 12 || second[0] != '\\' || second[1] != 'u')
        {
            return 0;
        }
        second_code = parse_hex4(second + 2);
        if (second_code < 0xdc00u || second_code > 0xdfffu)
        {
            return 0;
        }
        codepoint = 0x10000u + (((first_code & 0x3ffu) << 10) | (second_code & 0x3ffu));
    }
    else
    {
        sequence_length = 6;
        codepoint = first_code;
    }
    if (codepoint < 0x80u)
    {
        utf8_length = 1;
    }
    else if (codepoint < 0x800u)
    {
        utf8_length = 2;
        first_byte_mark = 0xc0u;
    }
    else if (codepoint < 0x10000u)
    {
        utf8_length = 3;
        first_byte_mark = 0xe0u;
    }
    else if (codepoint <= 0x10ffffu)
    {
        utf8_length = 4;
        first_byte_mark = 0xf0u;
    }
    else
    {
        return 0;
    }
    for (position = utf8_length - 1; position > 0; position--)
    {
        (*output)[position] = (unsigned char)((codepoint | 0x80u) & 0xbfu);
        codepoint >>= 6;
    }
    if (utf8_length > 1)
    {
        (*output)[0] = (unsigned char)((codepoint | first_byte_mark) & 0xffu);
    }
    else
    {
        (*output)[0] = (unsigned char)(codepoint & 0x7fu);
    }
    *output += utf8_length;
    return sequence_length;
}
static int scan_string(const unsigned char *input, unsigned length,
                       unsigned char *output, unsigned capacity)
{
    unsigned at = 1;
    unsigned used = 0;
    if (input == 0 || output == 0 || length < 2 || input[0] != '"')
    {
        return -1;
    }
    while (at < length)
    {
        unsigned char byte = input[at];
        if (byte == '"')
        {
            if (used >= capacity || at + 1 != length)
            {
                return -2;
            }
            output[used] = 0;
            return (int)used;
        }
        if (byte < 32 || used + 4 >= capacity)
        {
            return -3;
        }
        if (byte != '\\')
        {
            output[used++] = byte;
            at++;
            continue;
        }
        if (at + 1 >= length)
        {
            return -4;
        }
        byte = input[at + 1];
        if (byte == 'u')
        {
            unsigned char *destination = output + used;
            unsigned consumed = decode_literal(input + at, length - at, &destination);
            if (consumed == 0)
            {
                return -5;
            }
            used = (unsigned)(destination - output);
            at += consumed;
            continue;
        }
        if (byte == 'n')
        {
            byte = '\n';
        }
        else if (byte == 't')
        {
            byte = '\t';
        }
        else if (byte != '"' && byte != '\\' && byte != '/')
        {
            return -6;
        }
        output[used++] = byte;
        at += 2;
    }
    return -7;
}
static unsigned text_length(const unsigned char *text)
{
    unsigned length = 0;
    while (text[length] != 0)
    {
        length++;
    }
    return length;
}
static unsigned hash_bytes(const unsigned char *bytes, unsigned count)
{
    unsigned hash = 17;
    unsigned i = 0;
    for (i = 0; i < count; i++)
    {
        hash = (hash * 131u + bytes[i]) & 65535u;
    }
    return hash;
}
static void write_hex4(unsigned char *output, unsigned value)
{
    const unsigned char digits[17] = "0123456789abcdef";
    unsigned i = 0;
    for (i = 0; i < 4; i++)
    {
        unsigned shift = 12u - i * 4u;
        output[i] = digits[(value >> shift) & 15u];
    }
    output[4] = 0;
}
unsigned evaluate(unsigned seed)
{
    const unsigned char *inputs[6] = {
        (const unsigned char *)"\"plain\"",
        (const unsigned char *)"\"\\u00a3\"",
        (const unsigned char *)"\"\\ud83d\\ude42\"",
        (const unsigned char *)"\"x\\u20ac\\ny\"",
        (const unsigned char *)"\"\\ud800x\"",
        (const unsigned char *)"\"\\udc00\""
    };
    unsigned char output[64];
    unsigned char fragment[5];
    unsigned selected = seed % 6u;
    unsigned next = (selected + 2u) % 6u;
    int first = scan_string(inputs[selected], text_length(inputs[selected]), output, 64);
    unsigned hash = first < 0 ? (unsigned)(-first) : hash_bytes(output, (unsigned)first);
    int second = scan_string(inputs[next], text_length(inputs[next]), output, 64);
    unsigned extra = second < 0 ? (unsigned)(-second) : hash_bytes(output, (unsigned)second);
    unsigned probe = 0;
    write_hex4(fragment, 0x100u + seed * 17u);
    probe = parse_hex4(seed == 11u ? 0 : fragment);
    hash = (hash * 31u + extra + seed * 23u) & 65535u;
    return (hash + (probe & 255u)) & 65535u;
}
int main(void)
{
    unsigned result = evaluate(0);
    result ^= evaluate(1);
    result ^= evaluate(2);
    result ^= evaluate(11);
    return (int)(result & 255u);
}
