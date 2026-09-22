int main(void) {
    int n = 3, calls = 0;
    int a[n];
    __typeof__((calls++, &a)) p = &a;
    if (calls != 1 || sizeof(*p) != 3 * sizeof(int)) return 1;
    __typeof__(({ struct local_tag { int x; }; calls++; &a; })) q = &a;
    struct local_tag { long y; } t;
    t.y = 7;
    if (calls != 2 || sizeof(*q) != sizeof a || t.y != 7) return 2;
    __typeof__(calls++) scalar = 5;
    if (calls != 2 || scalar != 5) return 3;
    (*p)[1] = 9;
    return (*q)[1] != 9;
}
