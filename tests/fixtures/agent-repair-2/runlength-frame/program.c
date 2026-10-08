/* Run-length frames for byte arrays of length 0 through 128.
   A literal packet contains 1 through 128 bytes; a repeat packet contains
   3 through 66 copies. Frames carry the decoded length and a checksum.
   Short payloads use the low 16 checksum bits; longer ones use all 32.
   Buffers contain unsigned integers whose values are bytes, 0 through 255.
   evaluate accepts seeds 0 through 11 and exercises complete round trips. */

unsigned int frame_mask(int width)
{
    return (1U << width) - 1U;
}

unsigned int frame_checksum(const unsigned int *bytes, int count)
{
    unsigned int value = 2166136261U;
    int i;
    for (i = 0; i < count; i++) {
        value = (value ^ bytes[i]) * 16777619U;
        value ^= value >> 13;
    }
    return value & frame_mask(count < 32 ? 16 : 32);
}

int repeat_length(const unsigned int *source, int count, int start)
{
    int length = 1;
    while (start + length < count && length < 66) {
        if (source[start + length] != source[start]) {
            break;
        }
        length++;
    }
    return length;
}

int encode_runs(const unsigned int *source, int count,
                unsigned int *packet, int capacity)
{
    int input = 0;
    int output = 0;
    while (input < count) {
        int length = repeat_length(source, count, input);
        if (length >= 3) {
            if (output + 2 > capacity) {
                return -1;
            }
            packet[output++] = 128U + (unsigned int)(length - 2);
            packet[output++] = source[input];
            input += length;
        } else {
            int start = input;
            int literals = 0;
            while (input < count && literals < 128) {
                if (repeat_length(source, count, input) >= 3) {
                    break;
                }
                input++;
                literals++;
            }
            if (output + 1 + literals > capacity) {
                return -1;
            }
            packet[output++] = (unsigned int)(literals - 1);
            while (start < input) {
                packet[output++] = source[start++];
            }
        }
    }
    return output;
}

int decode_runs(const unsigned int *packet, int count,
                unsigned int *destination, int capacity)
{
    int input = 0;
    int output = 0;
    while (input < count) {
        unsigned int tag = packet[input++];
        int length;
        int i;
        if (tag < 128U) {
            length = (int)tag + 1;
            if (input + length > count || output + length > capacity) {
                return -1;
            }
            for (i = 0; i < length; i++) {
                destination[output++] = packet[input++];
            }
        } else {
            length = (int)(tag - 128U) + 3;
            if (length > 66 || input >= count || output + length > capacity) {
                return -1;
            }
            for (i = 0; i < length; i++) {
                destination[output++] = packet[input];
            }
            input++;
        }
    }
    return output;
}

int encode_frame(const unsigned int *source, int count,
                 unsigned int *frame, int capacity)
{
    int body;
    int i;
    unsigned int checksum;
    if (count < 0 || count > 128 || capacity < 7) {
        return -1;
    }
    for (i = 0; i < count; i++) {
        if (source[i] > 255U) {
            return -1;
        }
    }
    body = encode_runs(source, count, frame + 3, capacity - 7);
    if (body < 0) {
        return -1;
    }
    frame[0] = 82U;
    frame[1] = (unsigned int)count & 255U;
    frame[2] = (unsigned int)count >> 8;
    checksum = frame_checksum(source, count);
    for (i = 0; i < 4; i++) {
        frame[body + 3 + i] = (checksum >> (8 * i)) & 255U;
    }
    return body + 7;
}

int decode_frame(const unsigned int *frame, int count,
                 unsigned int *destination, int capacity)
{
    unsigned int checksum = 0U;
    int expected;
    int decoded;
    int i;
    if (count < 7 || frame[0] != 82U) {
        return -1;
    }
    expected = (int)(frame[1] | (frame[2] << 8));
    if (expected > 128 || expected > capacity) {
        return -1;
    }
    decoded = decode_runs(frame + 3, count - 7, destination, capacity);
    if (decoded != expected) {
        return -1;
    }
    for (i = 0; i < 4; i++) {
        checksum |= frame[count - 4 + i] << (8 * i);
    }
    if (frame_checksum(destination, decoded) != checksum) {
        return -1;
    }
    return decoded;
}

int make_frame_input(int seed, unsigned int *source)
{
    int count = 24 + seed * 4;
    int group = 3 + seed % 4;
    int i;
    for (i = 0; i < count; i++) {
        int block = i / group;
        if (block % 3 == 0) {
            source[i] = (unsigned int)(seed + block) & 255U;
        } else {
            source[i] = (unsigned int)(i * 17 + seed * 29) & 255U;
        }
    }
    return count;
}

int evaluate(int seed)
{
    unsigned int source[128];
    unsigned int frame[256];
    unsigned int decoded[128];
    unsigned int fingerprint;
    int count;
    int size;
    int recovered;
    int i;
    if (seed < 0 || seed > 11) {
        return -10;
    }
    count = make_frame_input(seed, source);
    size = encode_frame(source, count, frame, 256);
    if (size < 0) {
        return -20;
    }
    recovered = decode_frame(frame, size, decoded, 128);
    if (recovered != count) {
        return -30;
    }
    for (i = 0; i < count; i++) {
        if (source[i] != decoded[i]) {
            return -40;
        }
    }
    fingerprint = frame_checksum(source, count);
    fingerprint += 3U * frame_checksum(frame, size);
    fingerprint += (unsigned int)(size * 257 + count);
    return (int)(fingerprint & 2147483647U);
}

int main(void)
{
    int short_frame = evaluate(0);
    int second_frame = evaluate(1);
    int long_frame = evaluate(2);
    return short_frame < 0 || second_frame < 0 || long_frame < 0;
}
