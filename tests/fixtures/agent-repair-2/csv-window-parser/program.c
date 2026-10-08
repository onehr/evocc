/* Read comma-separated records with quoted fields and doubled quotes.
 * Columns are an identifier, a label and a nonnegative decimal quantity.
 * Decimal quantities may have underscores between digits. They saturate
 * at 2147483647. Empty or malformed quantities have value zero.
 */
typedef struct {
    const char *cursor;
    unsigned row;
    unsigned column;
    int error;
} Reader;

static unsigned length(const char *text)
{
    unsigned count = 0;
    while (text[count] != 0)
        ++count;
    return count;
}

static void append(char *buffer, unsigned *used, const char *text)
{
    unsigned i = 0;
    while (text[i] != 0 && *used < 511)
        buffer[(*used)++] = text[i++];
    buffer[*used] = 0;
}

static void decimal_text(char *output, unsigned value, int separated)
{
    char reverse[16];
    unsigned used = 0;
    unsigned written = 0;
    do {
        reverse[used++] = (char)('0' + value % 10);
        value /= 10;
    } while (value != 0);
    while (used != 0) {
        output[written++] = reverse[--used];
        if (separated && used != 0 && used % 3 == 0)
            output[written++] = '_';
    }
    output[written] = 0;
}

static int quantity(const char *text)
{
    const char *cursor = text;
    int value = 0;
    int saw_digit = 0;
    while (*cursor == ' ')
        ++cursor;
    while (*cursor != 0 && *cursor != ' ') {
        int digit;
        if (*cursor == '_') {
            if (!saw_digit || cursor[1] < '0' || cursor[1] > '9')
                return 0;
            cursor += 2;
            continue;
        }
        if (*cursor < '0' || *cursor > '9')
            return 0;
        digit = *cursor - '0';
        if (value > 2147483647 / 10)
            return 2147483647;
        value = value * 10 + digit;
        saw_digit = 1;
        ++cursor;
    }
    while (*cursor == ' ')
        ++cursor;
    return *cursor == 0 && saw_digit ? value : 0;
}

/* Output buffers hold at most 63 data bytes and one terminator. */
static int field(Reader *reader, char *output)
{
    const char *cursor = reader->cursor;
    unsigned used = 0;
    int quoted = *cursor == '"';
    int closed = !quoted;
    if (*cursor == 0)
        return 0;
    if (quoted)
        ++cursor;
    while (*cursor != 0) {
        char value = *cursor++;
        if (quoted && value == '"') {
            if (*cursor == '"')
                ++cursor;
            else {
                closed = 1;
                break;
            }
        } else if (!quoted && (value == ',' || value == '\n')) {
            --cursor;
            break;
        }
        if (used == 63) {
            reader->error = 1;
            return 0;
        }
        output[used++] = value;
    }
    output[used] = 0;
    if (!closed || (*cursor != ',' && *cursor != '\n' && *cursor != 0)) {
        reader->error = 1;
        return 0;
    }
    ++reader->column;
    if (*cursor == '\n') {
        ++reader->row;
        reader->column = 0;
    }
    if (*cursor != 0)
        ++cursor;
    reader->cursor = cursor;
    return 1;
}

static unsigned long long parse(const char *text)
{
    Reader reader;
    char identifier[64];
    char label[64];
    char amount[64];
    unsigned long long total = 0;
    reader.cursor = text;
    reader.row = 0;
    reader.column = 0;
    reader.error = 0;
    while (field(&reader, identifier)) {
        unsigned i;
        unsigned label_hash = 0;
        if (!field(&reader, label) || !field(&reader, amount))
            return 0xffffffffffffffffULL;
        for (i = 0; i < length(label); ++i)
            label_hash = label_hash * 33U + (unsigned char)label[i];
        total = total * 65599U + (unsigned)quantity(identifier);
        total ^= (unsigned long long)(unsigned)quantity(amount) * (1U + label_hash);
    }
    return reader.error ? 0xffffffffffffffffULL : total;
}

/* Every unsigned seed selects one of twelve distinct tables. */
unsigned long long evaluate(unsigned seed)
{
    char table[512];
    char number[32];
    unsigned used = 0;
    unsigned row;
    seed %= 12;
    table[0] = 0;
    for (row = 0; row < 5; ++row) {
        decimal_text(number, 100 + seed * 7 + row, 0);
        append(table, &used, number);
        append(table, &used, row & 1U ? ",\"north,west\"," : ",\"a\"\"b\",");
        decimal_text(number, 12345 + seed * 319 + row * 713, 1);
        append(table, &used, number);
        append(table, &used, "\n");
    }
    append(table, &used, "999,last,");
    append(table, &used, seed == 11 ? "2147483648\n" : "21474\n");
    return parse(table);
}

int main(void)
{
    unsigned long long checksum = evaluate(0);
    checksum ^= evaluate(1);
    checksum += evaluate(2);
    checksum ^= evaluate(11);
    return checksum == 0xffffffffffffffffULL;
}
