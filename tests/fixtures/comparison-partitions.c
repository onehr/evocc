extern void impossible_comparison(void);
void check(int x, int y) {
    if ((x == y) && (x != y)) impossible_comparison();
    if ((x < y) && (x > y)) impossible_comparison();
    if ((x < y) && (y < x)) impossible_comparison();
    if (!((x == y) || (x != y))) impossible_comparison();
    if (!((x >= y) || (x < y))) impossible_comparison();
    if (!((x <= y) || (y < x))) impossible_comparison();
}
int main(void) { check(0,0); check(1,2); check(4,3); return 0; }
