/* An in-memory record codec. The public evaluate function returns a fingerprint.
 * The command path reports the number of input bytes, not the fingerprint.
 * All buffers have explicit lengths. No module may read beyond those lengths.
 * Reset accepts an uninitialized automatic View. A prepared view owns its bytes.
 * The codec combines checksums, symbol counts, canonical codes, LZ sequences,
 * and a small JSON string parser and printer. It performs no I/O.
 */
typedef struct
{
    const unsigned char *data;
    unsigned char owned[32];
    unsigned length;
    unsigned seed;
} View;

typedef struct
{
    unsigned char bytes[256];
    unsigned size;
    unsigned position;
    unsigned error;
} Buffer;

typedef struct
{
    unsigned literal;
    unsigned distance;
    unsigned length;
} Sequence;

typedef struct
{
    unsigned count[32];
    unsigned maximum;
    unsigned distinct;
} Histogram;

typedef struct
{
    unsigned code[32];
    unsigned length[32];
    unsigned symbols;
    unsigned valid;
} Codebook;

typedef struct
{
    unsigned long long fingerprint;
    unsigned consumed;
    unsigned encoded;
} Result;

/* The buffer module uses the bounded-buffer approach of cJSON's printbuffer.
 * A failed write leaves size unchanged. The caller must inspect error.
 */
static void buffer_reset(Buffer *buffer)
{
    buffer->size = 0;
    buffer->position = 0;
    buffer->error = 0;
}

static void buffer_put(Buffer *buffer, unsigned byte)
{
    if (buffer->size == 256)
    {
        buffer->error = 1;
        return;
    }
    buffer->bytes[buffer->size] = (unsigned char)byte;
    ++buffer->size;
}

static unsigned buffer_get(Buffer *buffer)
{
    if (buffer->position >= buffer->size)
    {
        buffer->error = 1;
        return 0;
    }
    return buffer->bytes[buffer->position++];
}

static unsigned buffer_peek(const Buffer *buffer)
{
    if (buffer->position >= buffer->size)
    {
        return 0;
    }
    return buffer->bytes[buffer->position];
}

/* Whitespace and string escape handling adapt cJSON 1.7.19.
 * This bounded codec accepts ASCII strings and unsigned decimal integers.
 * It rejects unsupported escapes and does not claim to parse general JSON.
 */
static void skip_space(Buffer *input)
{
    while (input->position < input->size)
    {
        unsigned byte = buffer_peek(input);
        if (byte != ' ' && byte != '\n' && byte != '\r' && byte != '\t')
        {
            return;
        }
        ++input->position;
    }
}

static unsigned hex_value(unsigned byte)
{
    if (byte >= '0' && byte <= '9')
    {
        return byte - '0';
    }
    if (byte >= 'a' && byte <= 'f')
    {
        return byte - 'a' + 10;
    }
    if (byte >= 'A' && byte <= 'F')
    {
        return byte - 'A' + 10;
    }
    return 16;
}

static unsigned parse_hex4(Buffer *input)
{
    unsigned value = 0;
    unsigned index;
    for (index = 0; index < 4; ++index)
    {
        unsigned digit = hex_value(buffer_get(input));
        if (digit == 16)
        {
            input->error = 1;
            return 0;
        }
        value = value * 16 + digit;
    }
    return value;
}

static unsigned parse_string(Buffer *input, Buffer *output)
{
    skip_space(input);
    if (buffer_get(input) != '"')
    {
        input->error = 1;
        return 0;
    }
    while (input->position < input->size)
    {
        unsigned byte = buffer_get(input);
        if (byte == '"')
        {
            return !input->error && !output->error;
        }
        if (byte == '\\')
        {
            byte = buffer_get(input);
            switch (byte)
            {
                case 'b': byte = 8; break;
                case 'f': byte = 12; break;
                case 'n': byte = 10; break;
                case 'r': byte = 13; break;
                case 't': byte = 9; break;
                case '"': break;
                case '\\': break;
                case '/': break;
                case 'u':
                    byte = parse_hex4(input);
                    if (byte > 127)
                    {
                        input->error = 1;
                    }
                    break;
                default: input->error = 1; break;
            }
        }
        else if (byte < 32)
        {
            input->error = 1;
        }
        if (input->error)
        {
            return 0;
        }
        buffer_put(output, byte);
    }
    input->error = 1;
    return 0;
}

static void print_string(Buffer *output, const unsigned char *data, unsigned size)
{
    const unsigned char hex[] = "0123456789abcdef";
    unsigned index;
    buffer_put(output, '"');
    for (index = 0; index < size; ++index)
    {
        unsigned byte = data[index];
        switch (byte)
        {
            case '"':
            case '\\':
                buffer_put(output, '\\');
                buffer_put(output, byte);
                break;
            case '\n':
                buffer_put(output, '\\');
                buffer_put(output, 'n');
                break;
            case '\r':
                buffer_put(output, '\\');
                buffer_put(output, 'r');
                break;
            case '\t':
                buffer_put(output, '\\');
                buffer_put(output, 't');
                break;
            default:
                if (byte < 32)
                {
                    buffer_put(output, '\\');
                    buffer_put(output, 'u');
                    buffer_put(output, '0');
                    buffer_put(output, '0');
                    buffer_put(output, hex[byte >> 4]);
                    buffer_put(output, hex[byte & 15]);
                }
                else
                {
                    buffer_put(output, byte);
                }
                break;
        }
    }
    buffer_put(output, '"');
}

static void print_unsigned(Buffer *output, unsigned value)
{
    unsigned char digits[10];
    unsigned size = 0;
    do
    {
        digits[size++] = (unsigned char)('0' + value % 10);
        value /= 10;
    }
    while (value != 0);
    while (size != 0)
    {
        buffer_put(output, digits[--size]);
    }
}

static unsigned parse_unsigned(Buffer *input)
{
    unsigned value = 0;
    unsigned digits = 0;
    skip_space(input);
    while (buffer_peek(input) >= '0' && buffer_peek(input) <= '9')
    {
        unsigned digit = buffer_get(input) - '0';
        if (value > (4294967295u - digit) / 10u)
        {
            input->error = 1;
            return 0;
        }
        value = value * 10u + digit;
        ++digits;
    }
    if (digits == 0)
    {
        input->error = 1;
    }
    return value;
}

static void expect_byte(Buffer *input, unsigned expected)
{
    skip_space(input);
    if (buffer_get(input) != expected)
    {
        input->error = 1;
    }
}

/* View reset and access are shared by the checksum, match and count modules.
 * Every index passed to view_byte is less than the prepared view's length.
 */
static void view_reset(View *view, const unsigned char *input, unsigned seed)
{
    unsigned char temporary[32];
    unsigned index;
    for (index = 0; index < 32; ++index)
    {
        temporary[index] = input[index];
        view->owned[index] = input[index];
    }
    view->data = input;
    view->length = 32;
    view->seed = seed;
}

static unsigned char view_byte(const View *view, unsigned index)
{
    return view->data[index];
}

/* Adler-32 follows zlib 1.3.2 adler32_z: reduce both sums between blocks.
 * Keep the 5552-byte bound so the inner unsigned sums do not overflow.
 */
static unsigned adler_view(const View *view)
{
    unsigned first = view->seed & 65535u;
    unsigned second = view->seed >> 16;
    unsigned remaining = view->length;
    unsigned position = 0;
    while (remaining != 0)
    {
        unsigned block = remaining < 5552u ? remaining : 5552u;
        remaining -= block;
        while (block != 0)
        {
            first += view_byte(view, position++);
            second += first;
            --block;
        }
        first %= 65521u;
        second %= 65521u;
    }
    return first | (second << 16);
}

/* zlib's reflected CRC polynomial, with a compact nibble table.
 * Two lookups consume each byte. There is no lazy-initialization state.
 */
static unsigned crc_view(const View *view)
{
    unsigned table[16];
    unsigned symbol;
    unsigned crc = ~view->seed;
    for (symbol = 0; symbol < 16; ++symbol)
    {
        unsigned value = symbol;
        unsigned bit;
        for (bit = 0; bit < 4; ++bit)
        {
            value = value & 1u ? (value >> 1) ^ 0xedb88320u : value >> 1;
        }
        table[symbol] = value;
    }
    for (symbol = 0; symbol < view->length; ++symbol)
    {
        crc ^= view_byte(view, symbol);
        crc = table[crc & 15u] ^ (crc >> 4);
        crc = table[crc & 15u] ^ (crc >> 4);
    }
    return ~crc;
}

/* A byte-tail XXH32 adaptation from Zstandard 1.5.7's xxhash.h.
 * All arithmetic is unsigned modulo 2^32. The rotation count is fixed.
 */
static unsigned hash_view(const View *view)
{
    unsigned hash = view->seed + 374761393u + view->length;
    unsigned position = 0;
    while (position + 4 <= view->length)
    {
        unsigned value = view_byte(view, position);
        value |= (unsigned)view_byte(view, position + 1) << 8;
        value |= (unsigned)view_byte(view, position + 2) << 16;
        value |= (unsigned)view_byte(view, position + 3) << 24;
        hash += value * 3266489917u;
        hash = ((hash << 17) | (hash >> 15)) * 668265263u;
        position += 4;
    }
    while (position < view->length)
    {
        hash += view_byte(view, position++) * 374761393u;
        hash = ((hash << 11) | (hash >> 21)) * 2654435761u;
    }
    hash ^= hash >> 15;
    hash *= 2246822519u;
    hash ^= hash >> 13;
    hash *= 3266489917u;
    hash ^= hash >> 16;
    return hash;
}

/* The striped frequency counters follow Zstandard's HIST_count_parallel_wksp.
 * This variant counts the 32 low-bit symbols used by the prefix coder.
 */
static void histogram(const View *view, Histogram *result)
{
    unsigned stripes[4][32];
    unsigned index;
    unsigned lane;
    for (lane = 0; lane < 4; ++lane)
    {
        for (index = 0; index < 32; ++index)
        {
            stripes[lane][index] = 0;
        }
    }
    for (index = 0; index < view->length; ++index)
    {
        ++stripes[index & 3u][view_byte(view, index) & 31u];
    }
    result->maximum = 0;
    result->distinct = 0;
    for (index = 0; index < 32; ++index)
    {
        unsigned count = 0;
        for (lane = 0; lane < 4; ++lane)
        {
            count += stripes[lane][index];
        }
        result->count[index] = count;
        result->distinct += count != 0;
        if (count > result->maximum)
        {
            result->maximum = count;
        }
    }
}

/* Canonical codes use the length-count and next-code recurrence from zlib
 * trees.c. Reverse each code as DEFLATE writes its least significant bit first.
 * This module accepts at most 32 symbols and lengths in the range 0 through 8.
 */
static unsigned reverse_code(unsigned value, unsigned length)
{
    unsigned result = 0;
    while (length != 0)
    {
        result = (result << 1) | (value & 1u);
        value >>= 1;
        --length;
    }
    return result;
}

static void build_codes(Codebook *book, const unsigned *lengths, unsigned size)
{
    unsigned counts[9] = {0};
    unsigned next[9] = {0};
    unsigned code = 0;
    unsigned bits;
    unsigned symbol;
    book->valid = 0;
    book->symbols = 0;
    if (size > 32)
    {
        return;
    }
    for (symbol = 0; symbol < size; ++symbol)
    {
        if (lengths[symbol] > 8)
        {
            return;
        }
        if (lengths[symbol] != 0)
        {
            ++counts[lengths[symbol]];
        }
    }
    for (bits = 1; bits <= 8; ++bits)
    {
        code = (code + counts[bits - 1]) << 1;
        if (code + counts[bits] > (1u << bits))
        {
            return;
        }
        next[bits] = code;
    }
    for (symbol = 0; symbol < size; ++symbol)
    {
        bits = lengths[symbol];
        book->length[symbol] = bits;
        book->code[symbol] = bits ? reverse_code(next[bits]++, bits) : 0;
    }
    book->symbols = size;
    book->valid = 1;
}

static void pack_symbols(Buffer *output, const Codebook *book, const View *view)
{
    unsigned hold = 0;
    unsigned bits = 0;
    unsigned position;
    if (!book->valid || book->symbols != 32)
    {
        output->error = 1;
        return;
    }
    for (position = 0; position < view->length; ++position)
    {
        unsigned symbol = view_byte(view, position) & 31u;
        hold |= book->code[symbol] << bits;
        bits += book->length[symbol];
        while (bits >= 8)
        {
            buffer_put(output, hold & 255u);
            hold >>= 8;
            bits -= 8;
        }
    }
    if (bits != 0)
    {
        buffer_put(output, hold & 255u);
    }
}

/* A bounded LZ match search adapts Zstandard's greedy match and sequence split.
 * It scans earlier positions instead of using the upstream hash table.
 * Matches may overlap, as in the standard byte-at-a-time LZ copy operation.
 */
static unsigned match_length(const View *view, unsigned left, unsigned right)
{
    unsigned length = 0;
    while (right + length < view->length)
    {
        if (view_byte(view, left + length) != view_byte(view, right + length))
        {
            break;
        }
        ++length;
    }
    return length;
}

static unsigned find_sequences(const View *view, Sequence *sequences)
{
    unsigned position = 0;
    unsigned count = 0;
    while (position < view->length)
    {
        unsigned best = 0;
        unsigned distance = 0;
        unsigned candidate;
        for (candidate = 0; candidate < position; ++candidate)
        {
            unsigned length = match_length(view, candidate, position);
            if (length > best)
            {
                best = length;
                distance = position - candidate;
            }
        }
        if (best >= 3)
        {
            sequences[count].literal = 0;
            sequences[count].distance = distance;
            sequences[count].length = best;
            position += best;
        }
        else
        {
            sequences[count].literal = view_byte(view, position++);
            sequences[count].distance = 0;
            sequences[count].length = 1;
        }
        ++count;
    }
    return count;
}

static void unpack_sequences(Buffer *output, const Sequence *sequences, unsigned count)
{
    unsigned index;
    for (index = 0; index < count; ++index)
    {
        const Sequence *sequence = &sequences[index];
        unsigned length;
        if (sequence->distance == 0)
        {
            buffer_put(output, sequence->literal);
            continue;
        }
        if (sequence->distance > output->size)
        {
            output->error = 1;
            return;
        }
        for (length = 0; length < sequence->length; ++length)
        {
            unsigned byte = output->bytes[output->size - sequence->distance];
            buffer_put(output, byte);
            if (output->error)
            {
                return;
            }
        }
    }
}

/* Write [payload,sequence_count] with the same string escaping as cJSON.
 * The decoder checks the delimiters and consumes all trailing whitespace.
 */
static void write_record(Buffer *out, const Buffer *payload, unsigned count)
{
    buffer_put(out, '[');
    print_string(out, payload->bytes, payload->size);
    buffer_put(out, ',');
    print_unsigned(out, count);
    buffer_put(out, ']');
}

static unsigned read_record(Buffer *input, Buffer *payload)
{
    unsigned count;
    expect_byte(input, '[');
    parse_string(input, payload);
    expect_byte(input, ',');
    count = parse_unsigned(input);
    expect_byte(input, ']');
    skip_space(input);
    if (input->position != input->size)
    {
        input->error = 1;
    }
    return count;
}

/* Build input with runs, repeated phrases and JSON escapes.
 * Arithmetic on seed is unsigned, including for large public caller inputs.
 */
static void make_input(unsigned char *input, unsigned seed)
{
    const unsigned char alphabet[] = "abcdeabcde\n\t\"\\";
    unsigned index;
    for (index = 0; index < 32; ++index)
    {
        unsigned choice = ((index % 7u) + seed) % 10u;
        input[index] = alphabet[choice];
    }
}

static unsigned fold_buffer(const Buffer *buffer)
{
    unsigned hash = 2166136261u;
    unsigned index;
    for (index = 0; index < buffer->size; ++index)
    {
        hash = (hash ^ buffer->bytes[index]) * 16777619u;
    }
    return hash;
}

static Result process(unsigned seed)
{
    unsigned char input[32];
    View view;
    Histogram counts;
    Codebook book;
    Sequence sequences[32];
    Buffer packed;
    Buffer unpacked;
    Buffer record;
    Buffer decoded;
    unsigned lengths[32];
    unsigned index;
    unsigned sequence_count;
    unsigned decoded_count;
    unsigned adler;
    unsigned crc;
    unsigned hash;
    Result result;
    make_input(input, seed);
    view_reset(&view, input, seed);
    adler = adler_view(&view);
    crc = crc_view(&view);
    hash = hash_view(&view);
    histogram(&view, &counts);
    for (index = 0; index < 32; ++index)
    {
        lengths[index] = 5;
    }
    build_codes(&book, lengths, 32);
    buffer_reset(&packed);
    pack_symbols(&packed, &book, &view);
    sequence_count = find_sequences(&view, sequences);
    buffer_reset(&unpacked);
    unpack_sequences(&unpacked, sequences, sequence_count);
    buffer_reset(&record);
    write_record(&record, &unpacked, sequence_count);
    buffer_reset(&decoded);
    decoded_count = read_record(&record, &decoded);
    result.consumed = view.length;
    result.encoded = packed.size;
    result.fingerprint = ((unsigned long long)adler << 32) | crc;
    result.fingerprint ^= (unsigned long long)hash * 2654435761u;
    result.fingerprint += fold_buffer(&packed);
    result.fingerprint ^= (unsigned long long)fold_buffer(&decoded) << 17;
    result.fingerprint += counts.maximum + 257u * counts.distinct;
    result.fingerprint += decoded_count + fold_buffer(&record);
    if (packed.error || unpacked.error || record.error || decoded.error)
    {
        result.fingerprint = 0;
    }
    return result;
}

/* The public result must use every codec module, even when main needs only
 * the consumed length. A caller can use any unsigned seed.
 */
unsigned long long evaluate(unsigned seed)
{
    Result result = process(seed);
    return result.fingerprint;
}

int main(void)
{
    Result result = process(5);
    return (int)result.consumed;
}
