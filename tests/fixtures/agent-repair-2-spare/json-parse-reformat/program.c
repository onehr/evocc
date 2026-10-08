/* Altered cJSON source; MIT notice retained in ../LICENSE.cjson. */
/* Parse a JSON text into a caller-owned node pool, then print it in the
 * cJSON formatted style: object members on their own lines, indented by one
 * tab per depth, written as "key":<tab>value; arrays on one line with ", "
 * between elements. Whitespace between tokens is any byte from 1 to 32,
 * including the space. Numbers are integers; a value above 2147483647 in
 * magnitude saturates to +-2147483647, as cJSON saturates valueint. Strings
 * keep at most 15 bytes. Only evaluate seeds 0..11 are supported.
 */
typedef unsigned long size_t;
enum { cJSON_False = 1, cJSON_True = 2, cJSON_NULL = 4, cJSON_Number = 8, cJSON_String = 16,
       cJSON_Array = 32, cJSON_Object = 64 };
enum { POOL_SIZE = 40, STR_SIZE = 16, NESTING_LIMIT = 8 };

typedef struct cJSON cJSON;
struct cJSON {
    cJSON *next;
    cJSON *prev;
    cJSON *child;
    int type;
    int valueint;
    char string[STR_SIZE];
    char valuestring[STR_SIZE];
};
typedef struct { cJSON nodes[POOL_SIZE]; int used; } pool_t;
typedef struct {
    const unsigned char *content;
    size_t length;
    size_t offset;
    size_t depth;
    pool_t *pool;
} parse_buffer;
typedef struct { char *buffer; size_t length; size_t offset; size_t depth; } printbuffer;

static int can_read(const parse_buffer *b, size_t size) { return b->offset + size <= b->length; }
static int can_access_at_index(const parse_buffer *b, size_t index) { return b->offset + index < b->length; }
static const unsigned char *buffer_at_offset(const parse_buffer *b) { return b->content + b->offset; }

static cJSON *cJSON_New_Item(pool_t *pool)
{
    cJSON *node;
    int i;
    if (pool->used >= POOL_SIZE)
        return 0;
    node = &pool->nodes[pool->used++];
    node->next = node->prev = node->child = 0;
    node->type = 0;
    node->valueint = 0;
    for (i = 0; i < STR_SIZE; i++)
        node->string[i] = node->valuestring[i] = 0;
    return node;
}

static int same_text(const unsigned char *text, const char *word, size_t length)
{
    size_t i;
    for (i = 0; i < length; i++)
        if (text[i] != (unsigned char)word[i])
            return 0;
    return 1;
}

static parse_buffer *buffer_skip_whitespace(parse_buffer *buffer)
{
    if (!can_access_at_index(buffer, 0))
        return buffer;
    while (can_access_at_index(buffer, 0) && buffer_at_offset(buffer)[0] < 32)
        buffer->offset++;
    if (buffer->offset == buffer->length)
        buffer->offset--;
    return buffer;
}

/* An optional '-' and at least one digit; no fraction or exponent. */
static int parse_number(cJSON *item, parse_buffer *input_buffer)
{
    const unsigned char *text = buffer_at_offset(input_buffer);
    size_t i = text[0] == '-';
    int value = 0;
    if (!can_access_at_index(input_buffer, i) || text[i] < '0' || text[i] > '9')
        return 0;
    while (can_access_at_index(input_buffer, i) && text[i] >= '0' && text[i] <= '9') {
        int digit = text[i] - '0';
        value = value * 10 + digit;
        i++;
    }
    item->valueint = text[0] == '-' ? -value : value;
    item->type = cJSON_Number;
    input_buffer->offset += i;
    return 1;
}

/* Escapes: \" \\ \/ \b \f \n \r \t. A \u escape is rejected. */
static int parse_string(cJSON *item, parse_buffer *input_buffer)
{
    const unsigned char *input_pointer = buffer_at_offset(input_buffer) + 1;
    const unsigned char *input_end = buffer_at_offset(input_buffer) + 1;
    size_t out = 0;
    if (buffer_at_offset(input_buffer)[0] != '\"')
        return 0;
    while ((size_t)(input_end - input_buffer->content) < input_buffer->length && *input_end != '\"') {
        if (input_end[0] == '\\') {
            if ((size_t)(input_end + 1 - input_buffer->content) >= input_buffer->length)
                return 0;
            input_end++;
        }
        input_end++;
    }
    if ((size_t)(input_end - input_buffer->content) >= input_buffer->length || *input_end != '\"')
        return 0;
    while (input_pointer < input_end) {
        unsigned char c = *input_pointer++;
        if (c == '\\') {
            c = *input_pointer++;
            switch (c) {
            case 'b': c = '\b'; break;
            case 'f': c = '\f'; break;
            case 'n': c = '\n'; break;
            case 'r': c = '\r'; break;
            case 't': c = '\t'; break;
            case '\"': case '\\': case '/': break;
            default: return 0;
            }
        }
        if (out + 1 < STR_SIZE)
            item->valuestring[out++] = (char)c;
    }
    item->valuestring[out] = 0;
    item->type = cJSON_String;
    input_buffer->offset = (size_t)(input_end - input_buffer->content) + 1;
    return 1;
}

static int parse_value(cJSON *item, parse_buffer *input_buffer);

/* Arrays and objects share one loop; objects also parse "key":. */
static int parse_children(cJSON *item, parse_buffer *input_buffer, int is_object)
{
    cJSON *head = 0, *current_item = 0;
    unsigned char close = is_object ? '}' : ']';
    if (input_buffer->depth >= NESTING_LIMIT)
        return 0;
    input_buffer->depth++;
    input_buffer->offset++;
    buffer_skip_whitespace(input_buffer);
    if (can_access_at_index(input_buffer, 0) && buffer_at_offset(input_buffer)[0] == close)
        goto success;
    if (!can_access_at_index(input_buffer, 0)) {
        input_buffer->offset--;
        return 0;
    }
    input_buffer->offset--;
    do {
        cJSON *new_item = cJSON_New_Item(input_buffer->pool);
        if (new_item == 0)
            return 0;
        if (head == 0) {
            current_item = head = new_item;
        } else {
            current_item->next = new_item;
            new_item->prev = current_item;
            current_item = new_item;
        }
        if (!can_access_at_index(input_buffer, 1))
            return 0;
        input_buffer->offset++;
        buffer_skip_whitespace(input_buffer);
        if (is_object) {
            int i;
            if (!parse_string(current_item, input_buffer))
                return 0;
            buffer_skip_whitespace(input_buffer);
            for (i = 0; i < STR_SIZE; i++) {
                current_item->string[i] = current_item->valuestring[i];
                current_item->valuestring[i] = 0;
            }
            if (!can_access_at_index(input_buffer, 0) || buffer_at_offset(input_buffer)[0] != ':')
                return 0;
            input_buffer->offset++;
            buffer_skip_whitespace(input_buffer);
        }
        if (!parse_value(current_item, input_buffer))
            return 0;
        buffer_skip_whitespace(input_buffer);
    } while (can_access_at_index(input_buffer, 0) && buffer_at_offset(input_buffer)[0] == ',');
    if (!can_access_at_index(input_buffer, 0) || buffer_at_offset(input_buffer)[0] != close)
        return 0;
success:
    input_buffer->depth--;
    if (head != 0)
        head->prev = current_item;
    item->type = is_object ? cJSON_Object : cJSON_Array;
    item->child = head;
    input_buffer->offset++;
    return 1;
}

static int parse_value(cJSON *item, parse_buffer *input_buffer)
{
    const unsigned char *text = buffer_at_offset(input_buffer);
    if (can_read(input_buffer, 4) && same_text(text, "null", 4)) {
        item->type = cJSON_NULL;
        input_buffer->offset += 4;
        return 1;
    }
    if (can_read(input_buffer, 5) && same_text(text, "false", 5)) {
        item->type = cJSON_False;
        input_buffer->offset += 5;
        return 1;
    }
    if (can_read(input_buffer, 4) && same_text(text, "true", 4)) {
        item->type = cJSON_True;
        item->valueint = 1;
        input_buffer->offset += 4;
        return 1;
    }
    if (!can_access_at_index(input_buffer, 0))
        return 0;
    if (text[0] == '\"')
        return parse_string(item, input_buffer);
    if (text[0] == '-' || (text[0] >= '0' && text[0] <= '9'))
        return parse_number(item, input_buffer);
    if (text[0] == '[' || text[0] == '{')
        return parse_children(item, input_buffer, text[0] == '{');
    return 0;
}

/* Like cJSON_ParseWithOpts with require_null_terminated: the text must end
 * after the value and optional whitespace. *error_offset receives the stop
 * position on failure. */
static cJSON *cJSON_Parse(pool_t *pool, const char *value, size_t *error_offset)
{
    parse_buffer buffer;
    cJSON *item = cJSON_New_Item(pool);
    buffer.content = (const unsigned char *)value;
    buffer.length = 1;
    while (value[buffer.length - 1] != 0)
        buffer.length++;
    buffer.offset = 0;
    buffer.depth = 0;
    buffer.pool = pool;
    if (item != 0 && parse_value(item, buffer_skip_whitespace(&buffer))) {
        buffer_skip_whitespace(&buffer);
        if (buffer.offset < buffer.length && buffer_at_offset(&buffer)[0] == 0)
            return item;
    }
    *error_offset = buffer.offset < buffer.length ? buffer.offset : buffer.length - 1;
    return 0;
}

static int put(printbuffer *p, const char *text)
{
    while (*text) {
        if (p->offset + 1 >= p->length)
            return 0;
        p->buffer[p->offset++] = *text++;
    }
    p->buffer[p->offset] = 0;
    return 1;
}

static int put_tabs(printbuffer *p, size_t count)
{
    while (count-- > 0)
        if (!put(p, "\t"))
            return 0;
    return 1;
}

static int print_number(int value, printbuffer *p)
{
    char digits[12];
    int pos = 11;
    unsigned magnitude = value < 0 ? 0u - (unsigned)value : (unsigned)value;
    digits[pos] = 0;
    do {
        digits[--pos] = (char)('0' + magnitude % 10);
        magnitude /= 10;
    } while (magnitude != 0);
    if (value < 0)
        digits[--pos] = '-';
    return put(p, digits + pos);
}

static int print_string_ptr(const char *text, printbuffer *p)
{
    char escaped[3] = { '\\', 0, 0 };
    if (!put(p, "\""))
        return 0;
    for (; *text; text++) {
        char one[2] = { 0, 0 };
        switch (*text) {
        case '\"': case '\\': escaped[1] = *text; break;
        case '\n': escaped[1] = 'n'; break;
        case '\t': escaped[1] = 't'; break;
        case '\b': escaped[1] = 'b'; break;
        default: escaped[1] = 0; one[0] = *text; break;
        }
        if (!put(p, escaped[1] ? escaped : one))
            return 0;
    }
    return put(p, "\"");
}

static int print_value(const cJSON *item, printbuffer *p)
{
    const cJSON *current = item->child;
    switch (item->type) {
    case cJSON_NULL: return put(p, "null");
    case cJSON_False: return put(p, "false");
    case cJSON_True: return put(p, "true");
    case cJSON_Number: return print_number(item->valueint, p);
    case cJSON_String: return print_string_ptr(item->valuestring, p);
    case cJSON_Array:
        if (!put(p, "["))
            return 0;
        for (; current != 0; current = current->next)
            if (!print_value(current, p) || (current->next != 0 && !put(p, ", ")))
                return 0;
        return put(p, "]");
    case cJSON_Object:
        p->depth++;
        if (!put(p, "{\n"))
            return 0;
        for (; current != 0; current = current->next) {
            if (!put_tabs(p, p->depth) || !print_string_ptr(current->string, p) || !put(p, ":\t"))
                return 0;
            if (!print_value(current, p) || (current->next != 0 && !put(p, ",")) || !put(p, "\n"))
                return 0;
        }
        if (!put_tabs(p, p->depth - 1) || !put(p, "}"))
            return 0;
        p->depth--;
        return 1;
    default:
        return 0;
    }
}

static const char *const documents[12] = {
    "{\"a\":1,\"b\":[true,false,null],\"c\":\"x\"}",
    "{ \"id\": 42, \"list\": [ 1, 2, 3 ], \"ok\": true }",
    "[1,[2,[3,[4]]],{\"k\":{\"m\":{}}},-5]",
    "{\"max\":2147483647,\"min\":-2147483647,\"zero\":0}",
    "{\"v\":[12345678901,5],\"w\":\"big\"}",
    "{\n\t\"name\":\t\"ab\\tc\",\n\t\"deep\":\t[[[[]]]]\n}",
    "[ \"spaced\" , { \"k\" : [ ] } , -0 ]",
    "{\"n\":-99999999999999,\"s\":\"\\\"q\\\"\"}",
    "{\"esc\":\"a\\nb\",\"list\":[{},[],{\"z\":null}]}",
    "{\"x\": {\"y\": {\"z\": [10, 20]}}, \"t\": false}",
    "[\n\t-2147483647,\n\t\"tab\\/slash\"\n]",
    " {\"open\":[1,2,{\"q\":\"unterminated}]}"
};

unsigned long long evaluate(unsigned seed)
{
    pool_t pool;
    char text[400];
    printbuffer p;
    size_t error_offset = 0, i;
    unsigned long long acc = 0xCBF29CE484222325ULL;
    cJSON *root;
    pool.used = 0;
    root = cJSON_Parse(&pool, documents[seed], &error_offset);
    if (root == 0)
        return 0xE0000000ULL + error_offset;
    p.buffer = text;
    p.length = sizeof(text);
    p.offset = 0;
    p.depth = 0;
    if (!print_value(root, &p))
        return 0xF0000000ULL + p.offset;
    for (i = 0; i < p.offset; i++)
        acc = (acc ^ (unsigned char)text[i]) * 0x100000001B3ULL;
    return (acc ^ (unsigned)pool.used << 16 ^ p.offset) * 0x100000001B3ULL;
}

int main(void)
{
    unsigned long long checksum = evaluate(1);
    checksum ^= evaluate(4) * 3;
    checksum += evaluate(9);
    return (int)(checksum & 127);
}
