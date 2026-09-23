/* K10: exercise the real consecutive build-id helpers at urandom_read_iter.
 * A reference-built diagnostic, not an EvoCC-compiled benchmark object.
 * No libbpf, BTF, source patch, kernel rebuild, or generated skeleton is used. */
#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <linux/bpf.h>
#include <linux/perf_event.h>
#include <sched.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/syscall.h>
#include <unistd.h>

#define DEPTH 16
struct capture {
    int stack_id, length;
    struct bpf_stack_build_id frames[DEPTH];
};
static int bpf(int command, union bpf_attr *attr)
{
    return syscall(SYS_bpf, command, attr, sizeof(*attr));
}
static int map(int type, unsigned size, unsigned flags)
{
    union bpf_attr a = {.map_type = type, .key_size = 4, .value_size = size,
                       .max_entries = 128, .map_flags = flags};
    return bpf(BPF_MAP_CREATE, &a);
}
static int lookup(int fd, unsigned key, void *out)
{
    union bpf_attr a = {.map_fd = fd, .key = (uintptr_t)&key, .value = (uintptr_t)out};
    return bpf(BPF_MAP_LOOKUP_ELEM, &a);
}
#define INS(code_, dst_, src_, off_, imm_) \
    ((struct bpf_insn){.code = code_, .dst_reg = dst_, .src_reg = src_, .off = off_, .imm = imm_})
#define EMIT(c, d, s, o, i) code[n++] = INS(c, d, s, o, i)
#define MOV(d, s) EMIT(BPF_ALU64 | BPF_MOV | BPF_X, d, s, 0, 0)
#define IMM(d, i) EMIT(BPF_ALU64 | BPF_MOV | BPF_K, d, 0, 0, i)
#define CALL(i) EMIT(BPF_JMP | BPF_CALL, 0, 0, 0, i)
#define MAP(d, fd) do { EMIT(BPF_LD | BPF_DW | BPF_IMM, d, BPF_PSEUDO_MAP_FD, 0, fd); \
                       EMIT(0, 0, 0, 0, 0); } while (0)

static int load(int stack, int array, int reverse)
{
    struct bpf_insn code[64];
    int n = 0, pid_jump, null_jump;
    MOV(6, 1);
    CALL(BPF_FUNC_get_current_pid_tgid);
    EMIT(BPF_ALU64 | BPF_RSH | BPF_K, 0, 0, 0, 32);
    pid_jump = n;
    EMIT(BPF_JMP | BPF_JNE | BPF_K, 0, 0, 0, getpid());
    MAP(1, array);
    EMIT(BPF_ST | BPF_W | BPF_MEM, 10, 0, -4, 0);
    MOV(2, 10);
    EMIT(BPF_ALU64 | BPF_ADD | BPF_K, 2, 0, 0, -4);
    CALL(BPF_FUNC_map_lookup_elem);
    null_jump = n;
    EMIT(BPF_JMP | BPF_JEQ | BPF_K, 0, 0, 0, 0);
    MOV(7, 0);
    for (int step = 0; step < 2; ++step) {
        MOV(1, 6);
        if ((step == 0) != reverse) {
            MAP(2, stack);
            IMM(3, BPF_F_USER_STACK);
            CALL(BPF_FUNC_get_stackid);
            EMIT(BPF_STX | BPF_W | BPF_MEM, 7, 0, 0, 0);
        } else {
            MOV(2, 7);
            EMIT(BPF_ALU64 | BPF_ADD | BPF_K, 2, 0, 0, 8);
            IMM(3, sizeof(((struct capture *)0)->frames));
            IMM(4, BPF_F_USER_STACK | BPF_F_USER_BUILD_ID);
            CALL(BPF_FUNC_get_stack);
            EMIT(BPF_STX | BPF_W | BPF_MEM, 7, 0, 4, 0);
        }
    }
    code[pid_jump].off = n - pid_jump - 1;
    code[null_jump].off = n - null_jump - 1;
    IMM(0, 0);
    EMIT(BPF_JMP | BPF_EXIT, 0, 0, 0, 0);
    char log[16384] = {0};
    union bpf_attr a = {.prog_type = BPF_PROG_TYPE_KPROBE, .insn_cnt = n,
        .insns = (uintptr_t)code, .license = (uintptr_t)"GPL", .log_level = 1,
        .log_buf = (uintptr_t)log, .log_size = sizeof(log)};
    int fd = bpf(BPF_PROG_LOAD, &a);
    if (fd < 0) fprintf(stderr, "load errno=%d: %s\n", errno, log);
    return fd;
}

static int check(int reverse, int type)
{
    struct bpf_stack_build_id ids[DEPTH] = {0};
    struct capture result = {.stack_id = -1, .length = -1};
    const char target[] = "urandom_read_iter";
    struct perf_event_attr attr = {.type = type, .size = sizeof(attr),
        .config1 = (uintptr_t)target, .sample_period = 1, .wakeup_events = 1};
    int stack = map(BPF_MAP_TYPE_STACK_TRACE, sizeof(ids), BPF_F_STACK_BUILD_ID);
    int array = map(BPF_MAP_TYPE_ARRAY, sizeof(result), 0);
    int prog = -1, perf = -1, random = -1, ok = 0;
    if (stack < 0 || array < 0) goto out;
    prog = load(stack, array, reverse);
    if (prog < 0) goto out;
    perf = syscall(SYS_perf_event_open, &attr, -1, 0, -1, PERF_FLAG_FD_CLOEXEC);
    if (perf < 0 || ioctl(perf, PERF_EVENT_IOC_SET_BPF, prog) || ioctl(perf, PERF_EVENT_IOC_ENABLE, 0)) goto out;
    random = open("/dev/urandom", O_RDONLY);
    char bytes[32];
    if (random < 0 || read(random, bytes, sizeof(bytes)) != sizeof(bytes)) goto out;
    if (ioctl(perf, PERF_EVENT_IOC_DISABLE, 0) || lookup(array, 0, &result)
        || result.length <= 0 || result.stack_id < 0 || lookup(stack, result.stack_id, ids)) goto out;
    unsigned map_valid = 0, array_valid = 0, map_ip = 0, array_ip = 0;
    for (int i = 0; i < DEPTH; ++i) {
        map_valid += ids[i].status == BPF_STACK_BUILD_ID_VALID;
        array_valid += result.frames[i].status == BPF_STACK_BUILD_ID_VALID;
        map_ip += ids[i].status == BPF_STACK_BUILD_ID_IP;
        array_ip += result.frames[i].status == BPF_STACK_BUILD_ID_IP;
    }
    printf("# order=%s id=%d bytes=%d map(valid=%u ip=%u) array(valid=%u ip=%u)\n",
           reverse ? "get_stack first" : "get_stackid first", result.stack_id,
           result.length, map_valid, map_ip, array_valid, array_ip);
    ok = reverse ? array_valid > 0 && map_valid == 0 && map_ip > 0
                 : map_valid > 0 && array_valid == 0 && array_ip > 0;
out:
    if (!ok) printf("# diagnostic errno=%d stack_id=%d length=%d\n", errno, result.stack_id, result.length);
    if (random >= 0) close(random);
    if (perf >= 0) close(perf);
    if (prog >= 0) close(prog);
    if (array >= 0) close(array);
    if (stack >= 0) close(stack);
    return ok;
}

int main(void)
{
    puts("TAP version 13\n1..2");
    cpu_set_t cpus;
    CPU_ZERO(&cpus);
    CPU_SET(0, &cpus);
    int type = -1;
    FILE *file = fopen("/sys/bus/event_source/devices/kprobe/type", "r");
    if (file) { if (fscanf(file, "%d", &type) != 1) type = -1; fclose(file); }
    if (type < 0 || sched_setaffinity(0, sizeof(cpus), &cpus)) {
        puts("not ok 1 kprobe/cpu prerequisite\nnot ok 2 kprobe/cpu prerequisite");
        return 1;
    }
    int first = check(0, type), second = check(1, type);
    printf("%s 1 first get_stackid converts; following get_stack uses IPs\n", first ? "ok" : "not ok");
    printf("%s 2 first get_stack converts; following get_stackid uses IPs\n", second ? "ok" : "not ok");
    return !(first && second);
}
