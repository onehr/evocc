struct object { char tag; long number; int *pointer; };

int main(void) {
    int scalar;
    int *pointer;
    unsigned short array[7];
    struct object record;
    int explicit_value = 19;
    if (scalar || pointer || record.tag || record.number || record.pointer)
        return 1;
    for (unsigned i = 0; i < sizeof(array) / sizeof(array[0]); ++i)
        if (array[i]) return 2;
    const unsigned char *bytes = (const unsigned char *)&record;
    for (unsigned i = 0; i < sizeof(record); ++i)
        if (bytes[i]) return 3;
    for (int i = 0; i < 3; ++i) {
        int fresh;
        if (fresh) return 4;
        fresh = i + 1;
    }
    return explicit_value != 19;
}
