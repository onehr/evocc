/* K01 diagnostic, not an EvoCC language test. Publish two-instruction AArch64
 * code through the documented Linux sync-core interface. Readers acknowledge
 * each generation before the writer changes it. The separate continuous mode
 * patches the aligned MOV while readers execute; a writer-preferring rwlock
 * supplies a bounded grace period before retiring the published data. */
#define _GNU_SOURCE
#include <errno.h>
#include <linux/membarrier.h>
#include <pthread.h>
#include <sched.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/syscall.h>
#include <unistd.h>

#ifndef __aarch64__
#error This diagnostic contains AArch64 instruction words.
#endif

enum { ROUNDS = 512, READS = 64, MAX_READERS = 4 };
static atomic_uint generation;
static atomic_int finished;
static int (*entry)(void);
static int explicit_isb, continuous, published;
static pthread_rwlock_t grace;
struct reader {
    pthread_t thread;
    atomic_uint acknowledged;
    unsigned cpu, reads, missing, first_round, first_value, first_instruction;
};
static struct reader readers[MAX_READERS];

static void bind_cpu(unsigned cpu)
{
    cpu_set_t set;
    CPU_ZERO(&set);
    CPU_SET(cpu, &set);
    if (sched_setaffinity(0, sizeof(set), &set)) {
        perror("sched_setaffinity");
        exit(2);
    }
}

static void *read_code(void *argument)
{
    struct reader *r = argument;
    bind_cpu(r->cpu);
    if (continuous) {
        while (!atomic_load_explicit(&finished, memory_order_acquire)) {
            if (pthread_rwlock_rdlock(&grace)) exit(2);
            for (unsigned i = 0; i < READS; ++i) {
                if (explicit_isb) __asm__ volatile("isb" ::: "memory");
                unsigned actual = entry();
                if (actual > 1 || (actual && !published)) {
                    if (!r->missing) {
                        r->first_round = atomic_load_explicit(&generation, memory_order_relaxed);
                        r->first_value = actual;
                        r->first_instruction = *(volatile uint32_t *)entry;
                    }
                    ++r->missing;
                }
                ++r->reads;
            }
            if (pthread_rwlock_unlock(&grace)) exit(2);
            sched_yield();
        }
        return NULL;
    }
    for (unsigned round = 1; round <= ROUNDS; ++round) {
        while (atomic_load_explicit(&generation, memory_order_acquire) != round)
            sched_yield();
        if (explicit_isb)
            __asm__ volatile("isb" ::: "memory");
        unsigned expected = 1 + (round & 1);
        for (unsigned i = 0; i < READS; ++i) {
            unsigned actual = entry();
            if (actual != expected) {
                if (!r->missing) {
                    r->first_round = round;
                    r->first_value = actual;
                    r->first_instruction = *(volatile uint32_t *)entry;
                }
                ++r->missing;
            }
            ++r->reads;
        }
        atomic_store_explicit(&r->acknowledged, round, memory_order_release);
    }
    return NULL;
}

static void install_code(void *write_at, void *execute_at, unsigned value)
{
    ((volatile uint32_t *)write_at)[0] = 0x52800000 | (value << 5); /* mov w0, #value */
    __builtin___clear_cache(write_at, (char *)write_at + 8);
    if (write_at != execute_at) __builtin___clear_cache(execute_at, (char *)execute_at + 8);
    if (syscall(SYS_membarrier, MEMBARRIER_CMD_PRIVATE_EXPEDITED_SYNC_CORE, 0, 0)) {
        perror("sync-core");
        exit(2);
    }
}

int main(int argc, char **argv)
{
    (void)argc;
    setbuf(stdout, NULL);
    alarm(20);
    int aliases = strstr(argv[0], "alias") != NULL;
    explicit_isb = strstr(argv[0], "isb") != NULL;
    continuous = strstr(argv[0], "continuous") != NULL;
    printf("TAP version 13\n1..1\n");
    long commands = syscall(SYS_membarrier, MEMBARRIER_CMD_QUERY, 0, 0);
    if (commands < 0 || !(commands & MEMBARRIER_CMD_PRIVATE_EXPEDITED_SYNC_CORE)) {
        printf("ok 1 # SKIP private sync-core unavailable\n");
        return 4;
    }
    if (syscall(SYS_membarrier, MEMBARRIER_CMD_REGISTER_PRIVATE_EXPEDITED_SYNC_CORE, 0, 0)) {
        perror("register sync-core");
        return 2;
    }
    cpu_set_t cpus;
    if (sched_getaffinity(0, sizeof(cpus), &cpus)) return 2;
    unsigned count = 0;
    for (unsigned cpu = 0; cpu < CPU_SETSIZE && count < MAX_READERS; ++cpu)
        if (CPU_ISSET(cpu, &cpus)) readers[count++].cpu = cpu;
    if (count < 2) {
        printf("ok 1 # SKIP requires at least two available CPUs\n");
        return 4;
    }
    long page = sysconf(_SC_PAGESIZE);
    int fd = memfd_create("instruction-publication", MFD_CLOEXEC);
    if (fd < 0 || ftruncate(fd, page)) return 2;
    void *write_at = mmap(NULL, page, PROT_READ | PROT_WRITE | (aliases ? 0 : PROT_EXEC), MAP_SHARED, fd, 0);
    void *execute_at = aliases ? mmap(NULL, page, PROT_READ | PROT_EXEC, MAP_SHARED, fd, 0) : write_at;
    if (write_at == MAP_FAILED || execute_at == MAP_FAILED) return 2;
    entry = (int (*)(void))execute_at;
    ((volatile uint32_t *)write_at)[1] = 0xd65f03c0; /* ret */
    install_code(write_at, execute_at, 0);
    pthread_rwlockattr_t attr;
    if (pthread_rwlockattr_init(&attr)
        || pthread_rwlockattr_setkind_np(&attr, PTHREAD_RWLOCK_PREFER_WRITER_NONRECURSIVE_NP)
        || pthread_rwlock_init(&grace, &attr)
        || pthread_rwlockattr_destroy(&attr)) return 2;
    for (unsigned i = 0; i < count; ++i)
        if (pthread_create(&readers[i].thread, NULL, read_code, &readers[i])) return 2;
    bind_cpu(readers[0].cpu);
    for (unsigned round = 1; round <= ROUNDS; ++round) {
        if (continuous) {
            if (pthread_rwlock_wrlock(&grace)) return 2;
            published = 1;
            if (pthread_rwlock_unlock(&grace)) return 2;
            install_code(write_at, execute_at, 1);
            atomic_store_explicit(&generation, round, memory_order_relaxed);
            usleep(100);
            install_code(write_at, execute_at, 0);
            if (pthread_rwlock_wrlock(&grace)) return 2;
            published = 0;
            if (pthread_rwlock_unlock(&grace)) return 2;
            usleep(100);
            continue;
        }
        unsigned value = 1 + (round & 1);
        install_code(write_at, execute_at, value);
        atomic_store_explicit(&generation, round, memory_order_release);
        for (unsigned i = 0; i < count; ++i)
            while (atomic_load_explicit(&readers[i].acknowledged, memory_order_acquire) != round)
                sched_yield();
    }
    atomic_store_explicit(&finished, 1, memory_order_release);
    unsigned reads = 0, missing = 0;
    for (unsigned i = 0; i < count; ++i) {
        if (pthread_join(readers[i].thread, NULL)) return 2;
        reads += readers[i].reads;
        missing += readers[i].missing;
        printf("# reader=%u cpu=%u missing=%u first=%u/%u instruction=%08x\n", i, readers[i].cpu,
               readers[i].missing, readers[i].first_round, readers[i].first_value, readers[i].first_instruction);
    }
    if (pthread_rwlock_destroy(&grace)) return 2;
    if (aliases && munmap(execute_at, page)) return 2;
    if (munmap(write_at, page) || close(fd)) return 2;
    printf("# aliases=%d explicit_isb=%d continuous=%d readers=%u rounds=%u reads=%u missing=%u\n",
           aliases, explicit_isb, continuous, count, ROUNDS, reads, missing);
    int pass = !missing && (continuous ? reads > 0 : reads == count * ROUNDS * READS);
    printf("%s 1 every published instruction generation is executed\n", pass ? "ok" : "not ok");
    return pass ? 0 : 1;
}
