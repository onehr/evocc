void free(void *);
int main(void) {
    char *p = __builtin_malloc(16);
    if (!p) return 1;
    if (__builtin_strcpy(p, "abc") != p) return 2;
    if (p[0] != 'a' || p[1] != 'b' || p[2] != 'c' || p[3]) return 3;
    if (__builtin_printf("%s %d\n", p, (unsigned char)7) != 6) return 4;
    free(p);
    return 0;
}
