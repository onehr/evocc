_Static_assert(__alignof(long) == 8, "type alignment");
int main(void) {
    int side = 0;
    int __attribute__((aligned(32))) aligned;
    if (__alignof(side++) != 4 || side != 0) return 1;
    if (__alignof(aligned) != 32) return 2;
    if (__alignof__(aligned) != __alignof(aligned)) return 3;
    return 0;
}
