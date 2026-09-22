/* One ELF-placement probe, not a kernel source-specific workaround. */
#ifndef LATE_SECTION_ONLY
int late(int) __attribute__((section(".exit.text"), noinline));
#endif
int late(int x) { return x + 3; }
int late(int) __attribute__((section(".exit.text"), noinline));

int boot(int) __attribute__((section(".init.text"), aligned(64),
                            patchable_function_entry(4, 2), noinline));
int boot(int x)
{
    static void *const targets[] = { &&done };
    goto *targets[0];
done:
    return x + 7;
}
extern int boot_alias(int) __attribute__((alias("boot")));

static __attribute__((always_inline, section(".discard.inline"))) inline
int inlined(int x) { return x + 1; }

int ordinary(int x) { return inlined(x); }
int main(void)
{
    return boot(1) != 8 || boot_alias(2) != 9 || late(2) != 5 || ordinary(3) != 4;
}
