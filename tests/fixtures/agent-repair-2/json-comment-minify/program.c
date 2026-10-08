/* Altered cJSON source; MIT notice retained in ../LICENSE.cjson. */
/* Null documents are refused and report produced length zero.
 * Only evaluate seeds 0..11 are supported.
 */
static unsigned text_length(const char *text)
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
static int copy_text(char *out, unsigned capacity, const char *in)
{
    unsigned i = 0;
    unsigned length = text_length(in);
    if (out == 0 || in == 0 || length >= capacity)
    {
        return 0;
    }
    for (i = 0; i <= length; i++)
    {
        out[i] = in[i];
    }
    return 1;
}
static int wrap_document(char *out, unsigned capacity, const char *input,
                         unsigned ordinal)
{
    unsigned position = 0;
    if (out == 0 || capacity < 8 || ordinal >= 100u)
    {
        return 0;
    }
    if (!copy_text(out + 1, capacity - 1u, input))
    {
        return 0;
    }
    out[0] = '[';
    position = text_length(out);
    if (position + 6u >= capacity)
    {
        return 0;
    }
    out[position++] = '\n';
    out[position++] = ',';
    if (ordinal >= 10u)
    {
        out[position++] = (char)('0' + ordinal / 10u);
    }
    out[position++] = (char)('0' + ordinal % 10u);
    out[position++] = ']';
    out[position] = 0;
    return 1;
}
static void skip_oneline_comment(char **input)
{
    *input += 2;
    for (; (*input)[0] != 0; ++(*input))
    {
        if ((*input)[0] == '\n')
        {
            *input += 1;
            return;
        }
    }
}
static void skip_multiline_comment(char **input)
{
    *input += 2;
    for (; (*input)[0] != 0; ++(*input))
    {
        if ((*input)[0] == '*' && (*input)[1] == '/')
        {
            *input += 2;
            return;
        }
    }
}
static void minify_string(char **input, char **output)
{
    (*output)[0] = (*input)[0];
    *input += 1;
    *output += 1;
    for (; (*input)[0] != 0; ++(*input), ++(*output))
    {
        (*output)[0] = (*input)[0];
        if ((*input)[0] == '"')
        {
            (*output)[0] = '"';
            *input += 1;
            *output += 1;
            return;
        }
        else if ((*input)[0] == '\\' && (*input)[1] == '\\')
        {
            (*output)[1] = (*input)[1];
            *input += 1;
            *output += 1;
        }
    }
}
static unsigned minify_inplace(char *json)
{
    char *into = json;
    char *begin = json;
    while (json[0] != 0)
    {
        switch (json[0])
        {
            case ' ':
            case '\t':
            case '\r':
            case '\n':
                json++;
                break;
            case '/':
                if (json[1] == '/')
                {
                    skip_oneline_comment(&json);
                }
                else if (json[1] == '*')
                {
                    skip_multiline_comment(&json);
                }
                else
                {
                    json++;
                }
                break;
            case '"':
                minify_string(&json, &into);
                break;
            default:
                into[0] = json[0];
                json++;
                into++;
                break;
        }
    }
    *into = 0;
    return (unsigned)(into - begin);
}
static unsigned hash_text(const char *text)
{
    unsigned hash = 19;
    unsigned i = 0;
    while (text[i] != 0)
    {
        hash = (hash * 131u + (unsigned char)text[i]) & 65535u;
        i++;
    }
    return hash;
}
static unsigned structural_score(const char *text)
{
    unsigned opens = 0;
    unsigned closes = 0;
    unsigned quoted = 0;
    unsigned i = 0;
    int in_string = 0;
    for (i = 0; text[i] != 0; i++)
    {
        if (text[i] == '\\' && in_string && text[i + 1] != 0)
        {
            i++;
            continue;
        }
        if (text[i] == '"')
        {
            in_string = !in_string;
            quoted++;
        }
        else if (!in_string && (text[i] == '[' || text[i] == '{'))
        {
            opens++;
        }
        else if (!in_string && (text[i] == ']' || text[i] == '}'))
        {
            closes++;
        }
    }
    return opens * 17u + closes * 23u + quoted * 31u + (unsigned)in_string;
}
unsigned evaluate(unsigned seed)
{
    const char *inputs[6] = {
        " { \"a\" : 1, \"b\" : [ 2, 3 ] } ",
        "/* prefix */ {\"a\": \"space here\", \"b\": 4} // tail\n",
        " [ \"x\\\" y\", 5, \"z\" ] ",
        "{ \"slash\": \"//still text\", /* gone */ \"n\": 6 }",
        " \n {\"escaped\": \"a\\\" b\", \"n\": 7} \t",
        "/* empty */ [ ] // last"
    };
    char first[128];
    char second[128];
    unsigned selected = seed % 6u;
    unsigned next = (selected + 1u) % 6u;
    unsigned a = 0;
    unsigned b = 0;
    if (!wrap_document(first, 128, inputs[selected], seed % 100u))
    {
        return 1;
    }
    if (!wrap_document(second, 128, inputs[next], (seed + 3u) % 100u))
    {
        return 2;
    }
    a = minify_inplace(first);
    b = minify_inplace(second);
    a += structural_score(first);
    b += structural_score(second);
    a = (a * 31u + hash_text(first)) & 65535u;
    b = (b * 17u + hash_text(second)) & 65535u;
    a += minify_inplace(seed == 11u ? 0 : second);
    return (a + b + seed * 13u) & 65535u;
}
int main(void)
{
    unsigned result = evaluate(0);
    result ^= evaluate(1);
    result ^= evaluate(2);
    result ^= evaluate(11);
    return (int)(result & 255u);
}
