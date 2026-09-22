/* Reference-built platform harness; the two operations come from EvoCC. */
#include <signal.h>
#include <sys/wait.h>
#include <unistd.h>
extern double fp_env_add(double, double);
extern double fp_env_transport(double);
static unsigned long read_fpcr(void) {
    unsigned long value;
    __asm__ volatile("mrs %0, fpcr" : "=r"(value));
    return value;
}
int main(void) {
    if (read_fpcr() || fp_env_add(1.25, 2.5) != 3.75) return 1;
    const unsigned long masks[] = {1ul<<22, 1ul<<24, 1ul<<25, 1ul<<8};
    for (unsigned i = 0; i < sizeof masks/sizeof masks[0]; ++i) {
        pid_t child = fork();
        if (child < 0) return 2;
        if (!child) {
            __asm__ volatile("msr fpcr, %0; isb" :: "r"(masks[i]) : "memory");
            if (fp_env_transport(1.25) != 1.25 || read_fpcr() != masks[i]) _exit(3);
            volatile double result = fp_env_add(1.25, 2.5);
            (void)result;
            _exit(4);
        }
        int status;
        if (waitpid(child, &status, 0) != child || !WIFSIGNALED(status)
            || WTERMSIG(status) != SIGTRAP) return 5;
    }
    return read_fpcr() != 0;
}
