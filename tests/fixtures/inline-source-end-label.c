/* Reduced from store_fprobe_entry_data: source "end" is not the inline exit. */
static __attribute__((always_inline)) inline void store(int *out, int op)
{
    if (!out)
        return;
    for (int i = 0; i < 3; ++i) {
        switch (op) {
        case 0:
            goto end;
        default:
            ++*out;
            break;
        }
    }
end:
    *out += 5;
}

static __attribute__((always_inline)) inline int twice(int op)
{
    int result = 0;
    store(&result, op);
    if (op)
        goto end;
    store(&result, 1);
end:
    return result;
}

int main(void)
{
    store((int *)0, 1);
    return twice(0) != 13 || twice(1) != 8;
}
