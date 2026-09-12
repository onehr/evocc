static int object;
static int evaluations;
static void *address(void) { ++evaluations; return &object; }
int main(void) {
    void *result = __builtin_extract_return_addr(address());
    if (result != &object || evaluations != 1) return 1;
    if (__builtin_extract_return_addr((void *)0) != (void *)0) return 2;
    return 0;
}
