#include <dlfcn.h>
int main(int argc, char **argv) {
    if (argc != 2) return 1;
    void *handle = dlopen(argv[1], RTLD_NOW | RTLD_LOCAL);
    if (!handle) return 2;
    int (*entry)(void) = (int (*)(void))dlsym(handle, "readonly_entry");
    if (!entry) return 3;
    int result = entry();
    if (dlclose(handle) != 0) return 4;
    return result;
}
