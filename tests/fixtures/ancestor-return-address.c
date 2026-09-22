static void *expected;
__attribute__((noinline)) static int child(void) {
    return __builtin_return_address(1) != expected
        || !__builtin_frame_address(1);
}
__attribute__((noinline)) static int parent(void) {
    expected = __builtin_return_address(0);
    int result = child();
    return result;
}
int main(void) { return parent(); }
