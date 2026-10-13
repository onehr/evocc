/* Minimal /bin/sleep for the selected-program initramfs (whole seconds). */
#include <stdlib.h>
#include <unistd.h>

int main(int argc, char **argv)
{
    unsigned seconds = argc > 1 ? (unsigned)strtoul(argv[1], NULL, 10) : 0;
    while (seconds)
        seconds = sleep(seconds);
    return 0;
}
