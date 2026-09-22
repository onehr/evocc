/* Diagnostic companion TU, never a replacement benchmark or PASS oracle.
 * Sample the interrupted AArch64 PC/LR three times, then exit 77. Link with
 * an unchanged Csmith source at -O0 -g to locate its repeating execution path.
 * Sampling alone does not prove nontermination or defined behavior.
 */
#define _GNU_SOURCE
#include <signal.h>
#include <stdint.h>
#include <stdlib.h>
#include <sys/time.h>
#include <ucontext.h>
#include <unistd.h>

static volatile sig_atomic_t samples;

static char *hex(char *out, uintptr_t value)
{
    static const char digits[] = "0123456789abcdef";
    *out++ = '0';
    *out++ = 'x';
    for (int shift = 60; shift >= 0; shift -= 4)
        *out++ = digits[(value >> shift) & 15];
    return out;
}

static void sample(int signal_number, siginfo_t *information, void *opaque)
{
    (void)signal_number;
    (void)information;
    const ucontext_t *context = opaque;
    char line[40];
    char *end = hex(line, context->uc_mcontext.pc);
    *end++ = ' ';
    end = hex(end, context->uc_mcontext.regs[30]);
    *end++ = '\n';
    if (write(STDERR_FILENO, line, (size_t)(end - line)) != end - line)
        _exit(78);
    if (++samples == 3)
        _exit(77);
}

__attribute__((constructor))
static void start_sampling(void)
{
    struct sigaction action = {0};
    action.sa_flags = SA_SIGINFO;
    action.sa_sigaction = sample;
    sigemptyset(&action.sa_mask);
    const struct itimerval timer = {{0, 100000}, {0, 100000}};
    if (sigaction(SIGPROF, &action, NULL) || setitimer(ITIMER_PROF, &timer, NULL))
        _exit(78);
}
