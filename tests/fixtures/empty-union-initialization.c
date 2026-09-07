union U { int first; int words[4]; };
union P { void *pointer; unsigned char bytes[16]; };
struct S { int before; union U value; int after; };

int main(void) {
    union U local = {};
    struct S nested = {.before = 3, .value = {}, .after = 7};
    union U *literal = &(union U){};
    union P pointer = {};
    for (int i = 0; i < 4; ++i)
        if (local.words[i] || nested.value.words[i] || literal->words[i]) return 1;
    if (nested.before != 3 || nested.after != 7 || pointer.pointer) return 2;
    for (int i = 0; i < 16; ++i)
        if (pointer.bytes[i]) return 3;
    return 0;
}
