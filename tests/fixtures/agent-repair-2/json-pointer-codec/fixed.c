/* Altered cJSON source; MIT notice retained in ../LICENSE.cjson. */
typedef struct
{
    unsigned length;
    unsigned char data[32];
    unsigned capacity;
} Token;
static unsigned pointer_encoded_length(const unsigned char *string)
{
    unsigned length = 0;
    if (string == 0)
    {
        return 0;
    }
    for (; *string != 0; string++, length++)
    {
        if (*string == '~' || *string == '/')
        {
            length++;
        }
    }
    return length;
}
static int encode_pointer(Token *destination, const unsigned char *source)
{
    unsigned read = 0;
    unsigned write = 0;
    unsigned length = pointer_encoded_length(source);
    if (destination == 0 || source == 0)
    {
        return 0;
    }
    destination->length = 0;
    if (length >= destination->capacity)
    {
        return 0;
    }
    for (read = 0; source[read] != 0; read++, write++)
    {
        if (source[read] == '/')
        {
            destination->data[write] = '~';
            destination->data[write + 1] = '1';
            write++;
        }
        else if (source[read] == '~')
        {
            destination->data[write] = '~';
            destination->data[write + 1] = '0';
            write++;
        }
        else
        {
            destination->data[write] = source[read];
        }
    }
    destination->data[write] = 0;
    destination->length = write;
    return 1;
}
static int decode_pointer(Token *token)
{
    unsigned read = 0;
    unsigned write = 0;
    if (token == 0)
    {
        return 0;
    }
    for (; token->data[read] != 0; write++, read++)
    {
        if (token->data[read] == '~')
        {
            if (token->data[read + 1] == '0')
            {
                token->data[write] = '~';
            }
            else if (token->data[read + 1] == '1')
            {
                token->data[write] = '/';
            }
            else
            {
                return 0;
            }
            read++;
        }
        else
        {
            token->data[write] = token->data[read];
        }
    }
    token->data[write] = 0;
    token->length = write;
    return 1;
}
static unsigned hash_token(const Token *token)
{
    unsigned hash = token->length;
    unsigned i = 0;
    for (i = 0; i < token->length; i++)
    {
        hash = (hash * 131u + token->data[i]) & 65535u;
    }
    return hash;
}
static void clear_token(Token *token)
{
    unsigned i = 0;
    for (i = 0; i < 32; i++)
    {
        token->data[i] = 0;
    }
    token->length = 0;
    token->capacity = 32;
}
static int token_equal(const Token *token, const unsigned char *text)
{
    unsigned i = 0;
    for (i = 0; i < token->length; i++)
    {
        if (text[i] == 0 || token->data[i] != text[i])
        {
            return 0;
        }
    }
    return text[i] == 0;
}
static unsigned roundtrip(const unsigned char *text)
{
    Token token;
    unsigned encoded_hash = 0;
    clear_token(&token);
    if (!encode_pointer(&token, text))
    {
        return 3;
    }
    encoded_hash = hash_token(&token);
    if (!decode_pointer(&token))
    {
        return 5;
    }
    return (encoded_hash * 17u + hash_token(&token) +
            71u * (unsigned)token_equal(&token, text)) & 65535u;
}
unsigned evaluate(unsigned seed)
{
    unsigned char input[40];
    unsigned char secondary[20];
    unsigned count = seed == 11u ? 39u : 6u + seed % 9u;
    unsigned i = 0;
    unsigned first = 0;
    unsigned second = 0;
    for (i = 0; i < 40; i++)
    {
        input[i] = 0;
    }
    for (i = 0; i < 20; i++)
    {
        secondary[i] = 0;
    }
    for (i = 0; i < count; i++)
    {
        input[i] = (unsigned char)('a' + (i + seed) % 20u);
    }
    input[count / 2u] = '/';
    if (seed != 11u)
    {
        input[1] = '~';
    }
    secondary[0] = 'x';
    secondary[1] = '/';
    secondary[2] = '~';
    secondary[3] = (unsigned char)('0' + seed % 10u);
    secondary[4] = 'z';
    first = roundtrip(input);
    second = roundtrip(secondary);
    return (first * 31u + second + seed * 29u) & 65535u;
}
int main(void)
{
    unsigned result = evaluate(0);
    result ^= evaluate(1);
    result ^= evaluate(2);
    result ^= evaluate(11);
    return (int)(result & 255u);
}
