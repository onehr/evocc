/* Reference flags: -fno-omit-frame-pointer -fno-optimize-sibling-calls. */
__attribute__((noinline)) static int leaf(void *parent, void *grandparent, void *greatgrandparent,
                                        void *parent_return, void *grandparent_return) {
    volatile unsigned char local[32] __attribute__((aligned(64)));
    local[0] = 7;
    if (__builtin_frame_address(1) != parent) return 1;
    if (__builtin_frame_address(2) != grandparent) return 2;
    if (__builtin_frame_address(3) != greatgrandparent) return 5;
#ifdef CHECK_RETURN_CHAIN
    if (__builtin_return_address(1) != parent_return) return 3;
    if (__builtin_return_address(2) != grandparent_return) return 4;
#else
    (void)parent_return;
    (void)grandparent_return;
#endif
    return local[0] != 7;
}

__attribute__((noinline)) static int middle(void *parent, void *grandparent, void *parent_return) {
    return leaf(__builtin_frame_address(0), parent, grandparent,
                __builtin_return_address(0), parent_return);
}

__attribute__((noinline)) static int outer(void *parent) {
    return middle(__builtin_frame_address(0), parent, __builtin_return_address(0));
}

int main(void) { return outer(__builtin_frame_address(0)); }
