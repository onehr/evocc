/* Platform harness: an opaque call may change FPCR after earlier arithmetic. */
#include <signal.h>
#include <sys/wait.h>
#include <unistd.h>
extern double chain(double, double);
extern double loop(int, double);
extern double both(int, double);
extern double one(int, double);
extern double after_call(double);
extern double after_asm(double);
extern double after_asm_goto(double);
extern double transport(double);
static int change_environment;
void foreign(void)
{
    if (change_environment)
        __asm__ volatile ("msr fpcr, %0; isb" :: "r"(1UL << 22) : "memory");
}
int main(void)
{
    if (chain(2.0, 4.0) != 3.0 || loop(3, 2.0) != 31.0
        || both(1, 2.0) != 6.0 || both(0, 2.0) != 2.0
        || one(1, 2.0) != 6.0 || one(0, 2.0) != 4.0
        || after_call(2.0) != 6.0 || after_asm(2.0) != 6.0
        || after_asm_goto(2.0) != 6.0 || transport(2.0) != 2.0)
        return 1;
#ifdef CHECK_ADMISSION
    for (int test = 0; test < 3; ++test) {
        pid_t child = fork();
        if (child < 0) return 2;
        if (!child) {
            change_environment = 1;
            if (test == 0) {
                volatile double value = after_call(2.0);
                (void)value;
            } else {
                foreign();
                if (transport(2.0) != 2.0) _exit(3);
                volatile double value = one(test == 2, 2.0);
                (void)value;
            }
            _exit(4);
        }
        int status;
        if (waitpid(child, &status, 0) != child || !WIFSIGNALED(status)
            || WTERMSIG(status) != SIGTRAP)
            return 5;
    }
#endif
    return 0;
}
