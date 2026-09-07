unsigned int x[1] = {2};
unsigned int change(void) { x[0] |= 128; return 1; }
struct S { unsigned int value : 8; } s = {2};
unsigned int change_field(void) { s.value |= 128; return 1; }
int calls;
unsigned int *destination(void) { calls++; return x; }
int main(void) {
    x[0] |= change();
    if (x[0] != 131) return 1;
    s.value |= change_field();
    if (s.value != 131) return 2;
    x[0] = 2;
    *destination() |= change();
    if (x[0] != 131 || calls != 1) return 3;
    x[0] = 2;
    x[0] += (1 ? change() : 0);
    return x[0] != 131;
}
