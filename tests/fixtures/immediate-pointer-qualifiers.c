struct item { int value; };
static int read(int *p) { return *p; }
static int *convert(const int *p) { return p; }
static void *opaque(const volatile void *p) { return p; }
int main(void) {
    const int value = 17;
    const struct item item = { 23 };
    int *p = &value;
    p = convert(&value);
    struct item *q = &item;
    q = &item;
    void *r = opaque(&value);
    r = (const volatile void *)&value;
    return read(&value) != 17 || *p != 17 || q->value != 23 || r != &value;
}
