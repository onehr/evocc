/* Section placement is observable to the kernel linker, even if main returns 0. */
int boot_data __attribute__((section(".init.data"))) = 42;

int boot_function(void) __attribute__((section(".init.text"), noinline, used));
int boot_function(void)
{
    return boot_data;
}

int main(void)
{
    return boot_function() != 42;
}
