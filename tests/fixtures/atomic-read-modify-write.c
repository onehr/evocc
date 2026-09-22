static unsigned long value;
int main(void) {
    if (__sync_add_and_fetch(&value, 0xfedcba9876543211UL) != 0xfedcba9876543211UL) return 1;
    if (__sync_fetch_and_add(&value, 1) != 0xfedcba9876543211UL) return 2;
    if (__sync_sub_and_fetch(&value, 2) != 0xfedcba9876543210UL) return 3;
    if (!__sync_bool_compare_and_swap(&value, 0xfedcba9876543210UL, 7)) return 4;
    if (__sync_val_compare_and_swap(&value, 8, 9) != 7 || value != 7) return 5;
    return 0;
}
