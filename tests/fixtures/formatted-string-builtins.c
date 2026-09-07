int main(void) {
    char buffer[16];
    if (__builtin_sprintf(buffer, "%d:%s", (unsigned char)7, "abc") != 5) return 1;
    if (__builtin_strcmp(buffer, "7:abc")) return 2;
    if (__builtin_snprintf(buffer, 4, "%d", (short)-1234) != 5) return 3;
    if (__builtin_strcmp(buffer, "-12")) return 4;
    if (__builtin_strchr(buffer, '1') != buffer + 1) return 5;
    if (__builtin_strncpy(buffer, "z", 4) != buffer) return 6;
    if (buffer[0] != 'z' || buffer[1] || buffer[2] || buffer[3]) return 7;
    return 0;
}
