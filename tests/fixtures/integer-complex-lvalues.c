struct S { _Complex int value; };
static struct S a[2];
static int evaluations;
static struct S *get(void) { ++evaluations; return &a[1]; }
int main(void) {
    _Complex int local[2] = {};
    __real__ local[0] = 3;
    __imag__ local[0] = 4;
    __real__ get()->value = 7;
    __imag__ a[1].value = 9;
    int *part = &__real__ a[1].value;
    *part += 2;
    local[1] = -3;
    if (local[1] != -3) return 1;
    a[0].value = 2;
    if (a[0].value != 2) return 2;
    return local[0] != 3 + 4i || a[1].value != 9 + 9i || evaluations != 1;
}
