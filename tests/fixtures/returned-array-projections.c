/* Every array projection consumes the returned object in this full expression. */
struct item { int value; };
struct result { struct item items[2]; int matrix[2][2]; };
static int calls;
static struct result make(int x) {
    ++calls;
    return (struct result){{{x}, {x + 1}}, {{x, x + 1}, {x + 2, x + 3}}};
}
static int read_item(const struct item *p) { return p->value; }
int main(void) {
    if (make(3).items->value != 3 || calls != 1) return 1;
    if ((*make(4).items).value != 4 || calls != 2) return 2;
    if ((make(5).items + 1)->value != 6 || calls != 3) return 3;
    if (read_item(make(6).items) != 6 || calls != 4) return 4;
    if (make(7).matrix[1][1] != 10 || calls != 5) return 5;
    int condition = 1;
    if ((condition ? make(8) : make(9)).items->value != 8 || calls != 6) return 6;
    struct result saved = {0};
    if ((saved = make(10)).items->value != 10 || calls != 7) return 7;
    if (saved.items[1].value != 11) return 8;
    if (sizeof(make(11).items) != 2 * sizeof(struct item) || calls != 7) return 9;
    return 0;
}
