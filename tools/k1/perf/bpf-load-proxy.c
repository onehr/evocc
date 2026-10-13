/* Fixed BPF syscall workload for kernel instruction profiles.
 *
 * test_progs needs vmlinux BTF and its own build; the retained direct
 * virtconfig Images have neither. This program uses only raw bpf(2) calls:
 * it loads a socket filter whose bounded loop makes the verifier keep one
 * state per iteration at the loop head (JIT on), runs it with
 * BPF_PROG_TEST_RUN, and updates, looks up and deletes hash-map keys.
 * personality(0xffffffff) brackets the measured work so that a profiler can
 * snapshot at the arm64 entry __arm64_sys_arm64_personality. It is a proxy
 * for the BPF kernel paths, not the 970-program test_progs population.
 */
#define _GNU_SOURCE
#include <errno.h>
#include <linux/bpf.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/personality.h>
#include <sys/syscall.h>
#include <unistd.h>

#define LOADS 40
#define RUNS 400
#define KEYS 2000

static long bpf(int command, union bpf_attr *attr)
{
    return syscall(__NR_bpf, command, attr, sizeof(*attr));
}

static struct bpf_insn insn(uint8_t code, uint8_t dst, uint8_t src, int16_t off, int32_t imm)
{
    return (struct bpf_insn){ .code = code, .dst_reg = dst, .src_reg = src, .off = off, .imm = imm };
}

int main(void)
{
    struct bpf_insn program[] = {
        insn(BPF_LDX | BPF_W | BPF_MEM, BPF_REG_7, BPF_REG_1, 0, 0),  /* r7 = skb->len */
        insn(BPF_ALU64 | BPF_MOV | BPF_K, BPF_REG_0, 0, 0, 0),
        insn(BPF_ALU64 | BPF_MOV | BPF_K, BPF_REG_1, 0, 0, 0),
        insn(BPF_ALU64 | BPF_ADD | BPF_X, BPF_REG_0, BPF_REG_1, 0, 0), /* loop: r0 += r1 */
        insn(BPF_ALU64 | BPF_XOR | BPF_X, BPF_REG_0, BPF_REG_7, 0, 0),
        insn(BPF_ALU64 | BPF_ADD | BPF_K, BPF_REG_1, 0, 0, 1),
        insn(BPF_JMP | BPF_JLT | BPF_K, BPF_REG_1, 0, -4, 300),
        insn(BPF_ALU64 | BPF_AND | BPF_K, BPF_REG_0, 0, 0, 0xffff),
        insn(BPF_JMP | BPF_EXIT, 0, 0, 0, 0),
    };
    unsigned char packet[64] = { 0 };
    union bpf_attr attr;
    uint64_t checksum = 0;
    int map;

    personality(0xffffffff);
    for (int load = 0; load < LOADS; load++) {
        memset(&attr, 0, sizeof(attr));
        attr.prog_type = BPF_PROG_TYPE_SOCKET_FILTER;
        attr.insns = (uintptr_t)program;
        attr.insn_cnt = sizeof(program) / sizeof(program[0]);
        attr.license = (uintptr_t)"GPL";
        int prog = bpf(BPF_PROG_LOAD, &attr);
        if (prog < 0) {
            printf("BPF_PROXY load failed: %s\n", strerror(errno));
            return 1;
        }
        for (int run = 0; load == 0 && run < RUNS; run++) {
            memset(&attr, 0, sizeof(attr));
            attr.test.prog_fd = prog;
            attr.test.data_in = (uintptr_t)packet;
            attr.test.data_size_in = sizeof(packet);
            attr.test.repeat = 1;
            if (bpf(BPF_PROG_TEST_RUN, &attr)) {
                printf("BPF_PROXY run failed: %s\n", strerror(errno));
                return 1;
            }
            checksum += attr.test.retval;
        }
        close(prog);
    }
    memset(&attr, 0, sizeof(attr));
    attr.map_type = BPF_MAP_TYPE_HASH;
    attr.key_size = 4;
    attr.value_size = 8;
    attr.max_entries = KEYS;
    map = bpf(BPF_MAP_CREATE, &attr);
    if (map < 0) {
        printf("BPF_PROXY map failed: %s\n", strerror(errno));
        return 1;
    }
    for (uint32_t key = 0; key < KEYS; key++) {
        uint64_t value = key * 7ULL;
        memset(&attr, 0, sizeof(attr));
        attr.map_fd = map;
        attr.key = (uintptr_t)&key;
        attr.value = (uintptr_t)&value;
        if (bpf(BPF_MAP_UPDATE_ELEM, &attr)) {
            printf("BPF_PROXY update failed: %s\n", strerror(errno));
            return 1;
        }
    }
    for (uint32_t key = 0; key < KEYS; key++) {
        uint64_t value = 0;
        memset(&attr, 0, sizeof(attr));
        attr.map_fd = map;
        attr.key = (uintptr_t)&key;
        attr.value = (uintptr_t)&value;
        if (bpf(BPF_MAP_LOOKUP_ELEM, &attr)) {
            printf("BPF_PROXY lookup failed: %s\n", strerror(errno));
            return 1;
        }
        attr.value = 0; /* BPF_MAP_DELETE_ELEM rejects fields after key */
        if (bpf(BPF_MAP_DELETE_ELEM, &attr)) {
            printf("BPF_PROXY delete failed: %s\n", strerror(errno));
            return 1;
        }
        checksum += value;
    }
    close(map);
    personality(0xffffffff);
    printf("BPF_PROXY loads %d runs %d keys %d checksum %llu\n", LOADS, RUNS, KEYS,
           (unsigned long long)checksum);
    return 0;
}
