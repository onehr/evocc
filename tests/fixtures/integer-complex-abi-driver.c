extern _Complex short complex_short(_Complex short);
struct pair { _Complex char a, b; };
extern struct pair complex_pair(struct pair);
extern _Complex int complex_int(int, _Complex int, int);
extern _Complex long complex_long(long, long, long, long, long, long, long, _Complex long, long);
int main(void) {
    _Complex short a = complex_short(3 + 4i);
    _Complex int b = complex_int(2, 3 + 4i, 5);
    _Complex long c = complex_long(1, 2, 3, 4, 5, 6, 7, 8 + 9i, 10);
    struct pair d = complex_pair((struct pair){1 + 2i, 3 + 4i});
    return a != 5 + 7i || b != 10 + 4i || c != 46 + 9i
        || d.a != 3 + 5i || d.b != 2 + 2i;
}
