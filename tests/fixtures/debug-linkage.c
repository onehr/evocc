static int local_counter __attribute__((section(".data..percpu")));
int public_counter __attribute__((section(".data..percpu")));
int hidden_counter __attribute__((visibility("hidden")));
int protected_counter __attribute__((visibility("protected")));

static int local_function(int value)
{
    return value + local_counter;
}

int (*local_address(void))(int)
{
    return local_function;
}

int inspect_counters(void)
{
    return local_function(public_counter) + hidden_counter + protected_counter;
}
