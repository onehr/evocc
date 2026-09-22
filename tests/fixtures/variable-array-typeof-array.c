int main(void) {
    int n = 3, calls = 0;
    int a[n];
    __typeof__(*(calls++, &a)) b;
    if (calls != 1 || sizeof b != sizeof a) return 1;
    __typeof__(*({ struct local_tag { int x; }; calls++; &a; })) c;
    struct local_tag { long y; } t;
    t.y = 7;
    if (calls != 2 || sizeof c != sizeof a || t.y != 7) return 2;
    __typeof__(calls++) scalar = 5;
    if (calls != 2 || scalar != 5) return 3;
    b[1] = 9;
    c[1] = b[1];
    return c[1] != 9;
}
