static int check(int condition) {
    void *p = condition ? 0 : (const void *)0;
    p = condition ? (const void *)0 : 0;
    return p != 0;
}
int main(void) { return check(0) || check(1); }
