/* Frozen Kbuild function-tracing profile: option defaults and local overrides. */
__attribute__((noinline)) int ordinary(int x) { return x + 1; }
__attribute__((noinline, patchable_function_entry(0, 0)))
int notrace_entry(int x) { return x + 2; }
__attribute__((noinline, patchable_function_entry(3, 1)))
int local_override(int x) { return x + 3; }
__attribute__((noinline, aligned(64)))
int aligned_entry(int x) { return ordinary(x) + 3; }

int main(void)
{
    return ordinary(4) != 5 || notrace_entry(4) != 6
        || local_override(4) != 7 || aligned_entry(4) != 8;
}
