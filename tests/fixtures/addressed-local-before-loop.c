int x, b, d, v;
static int run(void) {
    int h = 5;
    x = h;
    for (; b;) {
        int *p = &h;
        (*p)++;
    }
    return d;
}
void wrapper(void) { v = run(); }
int main(void) { b = 0; d = 0; x = 0; wrapper(); return x != 5; }
