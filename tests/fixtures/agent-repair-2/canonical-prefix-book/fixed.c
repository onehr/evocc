/* Canonical prefix codebooks with at most eight symbols and code lengths 1..8.
   A zero length means the symbol is absent. Nonempty, prefix-free books may be
   incomplete; oversubscribed books are rejected. Symbol sequences have length
   0 through 160. Code bits are emitted most-significant first, while stream
   words are filled from low bit to high bit. Unused final-word bits are zero.
   evaluate accepts seeds 0 through 11 and verifies complete encode/decode paths. */

unsigned int prefix_mask(int width)
{
    return width == 32 ? ~0U : (1U << width) - 1U;
}

int build_prefix_book(const int *lengths, int symbols,
                      int *counts, unsigned int *first,
                      int *start, int *order, unsigned int *codes)
{
    unsigned int next[9];
    unsigned int code = 0U;
    int total = 0;
    int length;
    int symbol;
    if (symbols < 1 || symbols > 8) {
        return 0;
    }
    for (length = 0; length <= 8; length++) {
        counts[length] = 0;
        first[length] = 0U;
        start[length] = 0;
        next[length] = 0U;
    }
    for (symbol = 0; symbol < symbols; symbol++) {
        length = lengths[symbol];
        if (length < 0 || length > 8) {
            return 0;
        }
        codes[symbol] = 0U;
        if (length != 0) {
            counts[length]++;
            total++;
        }
    }
    if (total == 0) {
        return 0;
    }
    total = 0;
    for (length = 1; length <= 8; length++) {
        code = (code + (unsigned int)counts[length - 1]) << 1;
        if (code + (unsigned int)counts[length] > (1U << length)) {
            return 0;
        }
        first[length] = code;
        next[length] = code;
        start[length] = total;
        for (symbol = 0; symbol < symbols; symbol++) {
            if (lengths[symbol] == length) {
                order[total++] = symbol;
            }
        }
    }
    for (symbol = 0; symbol < symbols; symbol++) {
        length = lengths[symbol];
        if (length != 0) {
            codes[symbol] = next[length]++;
            if ((codes[symbol] & ~prefix_mask(length)) != 0U) {
                return 0;
            }
        }
    }
    return 1;
}

int encode_prefix(const int *source, int count,
                  const int *lengths, const unsigned int *codes, int symbols,
                  unsigned int *words, int capacity)
{
    int cursor = 0;
    int i;
    if (count < 0 || count > 160 || capacity < 1 || capacity > 64) {
        return -1;
    }
    for (i = 0; i < capacity; i++) {
        words[i] = 0U;
    }
    for (i = 0; i < count; i++) {
        int symbol = source[i];
        int length;
        int bit;
        if (symbol < 0 || symbol >= symbols) {
            return -1;
        }
        length = lengths[symbol];
        if (length < 1 || length > 8 || cursor + length > capacity * 32) {
            return -1;
        }
        for (bit = length - 1; bit >= 0; bit--) {
            unsigned int value = (codes[symbol] >> bit) & 1U;
            words[cursor / 32] |= value << (cursor % 32);
            cursor++;
        }
    }
    if (cursor != 0) {
        int tail = cursor % 32;
        words[(cursor - 1) / 32] &= prefix_mask(tail == 0 ? 32 : tail);
    }
    return cursor;
}

int decode_prefix(const unsigned int *words, int bits, int wanted,
                  const int *counts, const unsigned int *first,
                  const int *start, const int *order,
                  int *destination, int capacity)
{
    int cursor = 0;
    int output;
    if (wanted < 0 || wanted > capacity || bits < 0 || bits > 2048) {
        return -1;
    }
    for (output = 0; output < wanted; output++) {
        unsigned int code = 0U;
        int found = 0;
        int length;
        for (length = 1; length <= 8; length++) {
            unsigned int bit;
            if (cursor >= bits) {
                return -1;
            }
            bit = (words[cursor / 32] >> (cursor % 32)) & 1U;
            cursor++;
            code = (code << 1) | bit;
            if (counts[length] > 0 && code >= first[length] && code - first[length] < (unsigned int)counts[length]) {
                destination[output] = order[start[length] + (int)(code - first[length])];
                found = 1;
                break;
            }
        }
        if (!found) {
            return -1;
        }
    }
    if (cursor != bits) {
        return -1;
    }
    return wanted;
}

int make_prefix_input(int seed, int *source, int *lengths)
{
    unsigned int state = (unsigned int)(seed + 1) * 747796405U;
    int family = seed % 3;
    int active = family == 0 ? 8 : (family == 1 ? 6 : 4);
    int count = 8 + seed * 12;
    int i;
    for (i = 0; i < 8; i++) {
        lengths[i] = 0;
    }
    if (family == 0) {
        for (i = 0; i < 8; i++) {
            lengths[i] = 3;
        }
    } else if (family == 1) {
        lengths[0] = 1;
        lengths[1] = 3;
        lengths[2] = 3;
        lengths[3] = 3;
        lengths[4] = 4;
        lengths[5] = 4;
    } else {
        for (i = 0; i < 4; i++) {
            lengths[i] = 2;
        }
    }
    for (i = 0; i < count; i++) {
        state = state * 2891336453U + 277803737U;
        source[i] = (int)((state >> 16) % (unsigned int)active);
    }
    return count;
}

unsigned int prefix_signature(const int *symbols, int count,
                              const unsigned int *words, int bits)
{
    unsigned int signature = (unsigned int)bits + 2166136261U;
    int i;
    for (i = 0; i < count; i++) {
        signature = (signature ^ (unsigned int)symbols[i]) * 16777619U;
    }
    for (i = 0; i < (bits + 31) / 32; i++) {
        signature ^= words[i] + 2654435761U;
        signature = signature * 2246822519U + (unsigned int)i;
    }
    return signature;
}

int evaluate(int seed)
{
    int source[160];
    int decoded[160];
    int lengths[8];
    int counts[9];
    int starts[9];
    int order[8];
    unsigned int codes[8];
    unsigned int first[9];
    unsigned int words[32];
    int count;
    int bits;
    int recovered;
    int i;
    if (seed < 0 || seed > 11) {
        return -10;
    }
    count = make_prefix_input(seed, source, lengths);
    if (!build_prefix_book(lengths, 8, counts, first, starts, order, codes)) {
        return -20;
    }
    bits = encode_prefix(source, count, lengths, codes, 8, words, 32);
    if (bits < 0) {
        return -30;
    }
    recovered = decode_prefix(words, bits, count, counts, first, starts, order, decoded, 160);
    if (recovered != count) {
        return -40;
    }
    for (i = 0; i < count; i++) {
        if (source[i] != decoded[i]) {
            return -50;
        }
    }
    return (int)(prefix_signature(decoded, count, words, bits) & 2147483647U);
}

int main(void)
{
    int partial_word = evaluate(0);
    int another_partial_word = evaluate(3);
    int complete_words = evaluate(2);
    return partial_word < 0 || another_partial_word < 0 || complete_words < 0;
}
