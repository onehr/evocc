static char (*saved)[4] = &"abc";
static char (*saved_end)[4] = &"xyz" + 1;

int main(void) {
    char (*array)[4] = &"abc";
    char (*embedded)[4] = &"a\0b";
    char (*joined)[3] = &"\x1" "2";
    unsigned short (*wide)[3] = &u"xy";
    char copy[4];
    __builtin_memcpy(copy, &"abc", 4);
    if (sizeof(*array) != 4 || (*array)[1] != 'b') return 1;
    if ((char *)(array + 1) - (char *)array != 4) return 2;
    if ((*embedded)[1] != 0 || (*embedded)[2] != 'b') return 3;
    if (sizeof(*joined) != 3 || (*joined)[0] != 1 || (*joined)[1] != '2') return 4;
    if (sizeof(*wide) != 6 || (*wide)[1] != 'y') return 5;
    if ((*saved)[2] != 'c' || (*(saved_end - 1))[0] != 'x') return 6;
    if (sizeof(*(&"abc")) != 4 || (*(&"abc"))[1] != 'b') return 7;
    if (copy[0] != 'a' || copy[2] != 'c' || copy[3] != 0) return 8;
    return 0;
}
