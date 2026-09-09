int vsnprintf(char *, unsigned long, const char *, __builtin_va_list);

int wrap(char *buf, unsigned n, const char *fmt, ...) {
    __builtin_va_list ap;
    __builtin_va_start(ap, fmt);
    int result = vsnprintf(buf, n, fmt, ap);
    __builtin_va_end(ap);
    return result;
}
