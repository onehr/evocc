typedef unsigned char v1 __attribute__((vector_size(1)));
typedef unsigned char v2 __attribute__((vector_size(2)));
typedef unsigned char v4 __attribute__((vector_size(4)));
extern v1 small1(v1);
extern v2 small2(v2);
extern v4 small4(v4);
int main(void) {
    v1 a = small1((v1){9});
    v2 b = small2((v2){9, 19});
    v4 c = small4((v4){9, 19, 29, 39});
    return a[0] != 10 || b[0] != 10 || b[1] != 20 || c[0] != 10 || c[3] != 40;
}
