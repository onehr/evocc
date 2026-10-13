/* L09 probe: what does __builtin_return_address(N) give for N = 1 and 2?
 * Exit value: 0 = the ancestor return address equals the caller's own
 * depth-zero value, 1 = a null pointer, 2 = any other value. Bit 2 (4) is set
 * when __builtin_frame_address(1) differs from the caller's frame address.
 * CASE=0 depth 1, CASE=1 depth 2. */
static void *expected;
static void *expected_frame;

__attribute__((noinline)) static int classify(void *got)
{
    return got == expected ? 0 : got == 0 ? 1 : 2;
}

#if CASE == 0
__attribute__((noinline)) static int child(void)
{
    int code = classify(__builtin_return_address(1));
    return code | ((__builtin_frame_address(1) != expected_frame) << 2);
}
__attribute__((noinline)) static int parent(void)
{
    expected = __builtin_return_address(0);
    expected_frame = __builtin_frame_address(0);
    int result = child();
    return result;
}
#elif CASE == 1
__attribute__((noinline)) static int grandchild(void)
{
    return classify(__builtin_return_address(2));
}
__attribute__((noinline)) static int child(void)
{
    int result = grandchild();
    return result;
}
__attribute__((noinline)) static int parent(void)
{
    expected = __builtin_return_address(0);
    int result = child();
    return result;
}
#endif

int main(void)
{
    int result = parent();
    return result;
}
