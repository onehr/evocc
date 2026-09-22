/* C11 temporary aggregate arrays decay for reads in the full expression. */
struct inner { int a[2]; };
struct value { struct inner nested; int a[2]; };
static int calls;
static struct value make(int x) {
    ++calls;
    return (struct value){{{x, x + 1}}, {x + 2, x + 3}};
}
static int sum(const int *p) { return p[0] + p[1]; }
int main(void) {
    if (make(10).a[1] != 13 || calls != 1) return 1;
    if (make(20).nested.a[0] != 20 || calls != 2) return 2;
    if (sum(make(30).a) != 65 || calls != 3) return 3;
    if ((make(40), make(50)).a[0] != 52 || calls != 5) return 4;
    int take = 1;
    if ((take ? make(60) : make(70)).a[1] != 63 || calls != 6) return 5;
    struct value v = {{{0, 0}}, {0, 0}};
    if ((v = make(80)).a[1] != 83 || calls != 7 || v.a[0] != 82) return 6;
    return 0;
}
