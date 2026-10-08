/* Variable-width integer packets, stored least-significant bit first.
   The 32-bit header contains a marker, a record count, and an extension flag.
   An extended packet carries a full-width sequence number before its records.
   Each record carries a five-bit width-minus-one and a value of that width.
   Counts are 0 through 16, widths are 1 through 32, and capacities are in words.
   evaluate accepts seeds 0 through 11 and checks values, widths, and metadata. */

unsigned int packet_mask(int width)
{
    return (1U << width) - 1U;
}

int write_field(unsigned int *words, int capacity, int *cursor,
                unsigned int value, int width)
{
    int remaining = width;
    int consumed = 0;
    if (width < 1 || width > 32 || capacity < 0 || capacity > 64) {
        return 0;
    }
    if (*cursor < 0 || *cursor + width > capacity * 32) {
        return 0;
    }
    if (width < 32 && value > packet_mask(width)) {
        return 0;
    }
    while (remaining > 0) {
        int offset = *cursor % 32;
        int word = *cursor / 32;
        int take = 32 - offset;
        unsigned int mask;
        unsigned int part;
        if (take > remaining) {
            take = remaining;
        }
        mask = packet_mask(take);
        part = (value >> consumed) & mask;
        words[word] = (words[word] & ~(mask << offset)) | (part << offset);
        *cursor += take;
        consumed += take;
        remaining -= take;
    }
    return 1;
}

int read_field(const unsigned int *words, int limit, int *cursor,
               unsigned int *value, int width)
{
    int remaining = width;
    int shift = 0;
    unsigned int result = 0U;
    if (width < 1 || width > 32 || *cursor < 0 || *cursor + width > limit) {
        return 0;
    }
    while (remaining > 0) {
        int offset = *cursor % 32;
        int take = 32 - offset;
        unsigned int part;
        if (take > remaining) {
            take = remaining;
        }
        part = (words[*cursor / 32] >> offset) & packet_mask(take);
        result |= part << shift;
        *cursor += take;
        remaining -= take;
        shift += remaining;
    }
    *value = result;
    return 1;
}

int encode_packet(const unsigned int *values, const int *widths, int count,
                  unsigned int sequence, int extended,
                  unsigned int *words, int capacity)
{
    int cursor = 0;
    int i;
    if (count < 0 || count > 16 || (extended != 0 && extended != 1)) {
        return -1;
    }
    if (capacity < 1 || capacity > 64) {
        return -1;
    }
    for (i = 0; i < capacity; i++) {
        words[i] = 0U;
    }
    if (!write_field(words, capacity, &cursor, 19792U, 16)) {
        return -1;
    }
    if (!write_field(words, capacity, &cursor, (unsigned int)count, 8)) {
        return -1;
    }
    if (!write_field(words, capacity, &cursor, (unsigned int)extended, 8)) {
        return -1;
    }
    if (extended && !write_field(words, capacity, &cursor, sequence, 32)) {
        return -1;
    }
    for (i = 0; i < count; i++) {
        if (widths[i] < 1 || widths[i] > 32) {
            return -1;
        }
        if (!write_field(words, capacity, &cursor, (unsigned int)(widths[i] - 1), 5)) {
            return -1;
        }
        if (!write_field(words, capacity, &cursor, values[i], widths[i])) {
            return -1;
        }
    }
    return cursor;
}

int decode_packet(const unsigned int *words, int bits,
                  unsigned int *values, int *widths, int capacity,
                  unsigned int *sequence)
{
    unsigned int marker;
    unsigned int count;
    unsigned int extended;
    int cursor = 0;
    int i;
    if (bits < 32 || bits > 2048) {
        return -1;
    }
    if (!read_field(words, bits, &cursor, &marker, 16) || marker != 19792U) {
        return -1;
    }
    if (!read_field(words, bits, &cursor, &count, 8) || count > 16U) {
        return -1;
    }
    if ((int)count > capacity) {
        return -1;
    }
    if (!read_field(words, bits, &cursor, &extended, 8) || extended > 1U) {
        return -1;
    }
    *sequence = 0U;
    if (extended && !read_field(words, bits, &cursor, sequence, 32)) {
        return -1;
    }
    for (i = 0; i < (int)count; i++) {
        unsigned int encoded_width;
        if (!read_field(words, bits, &cursor, &encoded_width, 5)) {
            return -1;
        }
        widths[i] = (int)encoded_width + 1;
        if (!read_field(words, bits, &cursor, values + i, widths[i])) {
            return -1;
        }
    }
    if (cursor != bits) {
        return -1;
    }
    return (int)count;
}

unsigned int packet_signature(const unsigned int *values, const int *widths,
                              int count, unsigned int sequence)
{
    unsigned int result = sequence ^ 2166136261U;
    int i;
    for (i = 0; i < count; i++) {
        result = (result ^ values[i]) * 16777619U;
        result += (unsigned int)(widths[i] * (i + 1));
    }
    return result;
}

int make_packet_input(int seed, unsigned int *values, int *widths)
{
    unsigned int state = (unsigned int)(seed + 1) * 2654435761U;
    int count = 4 + seed % 9;
    int i;
    for (i = 0; i < count; i++) {
        int width = 3 + (seed * 5 + i * 7) % 27;
        state = state * 1664525U + 1013904223U;
        widths[i] = width;
        values[i] = (state & packet_mask(width)) | (1U << (width - 1));
    }
    return count;
}

int evaluate(int seed)
{
    unsigned int values[16];
    unsigned int decoded[16];
    unsigned int words[32];
    int widths[16];
    int decoded_widths[16];
    unsigned int sequence;
    unsigned int recovered_sequence;
    unsigned int signature;
    int extended;
    int count;
    int bits;
    int recovered;
    int i;
    if (seed < 0 || seed > 11) {
        return -10;
    }
    extended = seed % 3 == 2;
    sequence = extended ? 2166572391U + (unsigned int)seed * 65537U : 0U;
    count = make_packet_input(seed, values, widths);
    bits = encode_packet(values, widths, count, sequence, extended, words, 32);
    if (bits < 0) {
        return -20;
    }
    recovered = decode_packet(words, bits, decoded, decoded_widths, 16, &recovered_sequence);
    if (recovered != count || recovered_sequence != sequence) {
        return -30;
    }
    for (i = 0; i < count; i++) {
        if (values[i] != decoded[i] || widths[i] != decoded_widths[i]) {
            return -40;
        }
    }
    signature = packet_signature(decoded, decoded_widths, count, recovered_sequence);
    signature ^= (unsigned int)(bits * 4099);
    return (int)(signature & 2147483647U);
}

int main(void)
{
    int compact = evaluate(0);
    int second_compact = evaluate(1);
    int extended = evaluate(2);
    return compact < 0 || second_compact < 0 || extended < 0;
}
