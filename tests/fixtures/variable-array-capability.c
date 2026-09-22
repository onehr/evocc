#if defined(__STDC_NO_VLA__) && __STDC_NO_VLA__
#error automatic variable-length arrays are supported
#endif
static unsigned long size(int n) {
    int a[n];
    a[n - 1] = 7;
    return sizeof a + a[n - 1];
}
int main(void) { return size(3) != 19 || size(7) != 35; }
