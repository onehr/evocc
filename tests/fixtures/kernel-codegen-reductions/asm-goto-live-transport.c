/* Each asm-goto join copies every live value, through stack slots.
 *
 * Kernel static keys, tracepoints and arm64 capability checks are asm goto.
 * Reduced from arch/arm64/kvm/mmu.c user_mem_abort (about 3,650 EvoCC
 * instructions per stage-2 fault, two thirds of them ldr/str/mov) and
 * mm/slub.c kmem_cache_alloc_noprof. GCC emits each static branch as one
 * patchable nop and keeps live values where they are. EvoCC passes all
 * twenty live values as block arguments to both asm-goto successors and to
 * the join; spilled ones get new slots, so each edge reloads, moves and
 * stores them again: about 140 extra instructions per disabled key here. It
 * also sets the branch result to 0 or 1 and tests it again. The hot path has
 * all keys disabled.
 * Measured (reduction-counts.py, 100,000 calls): EvoCC 488 and GCC 67
 * instructions per call.
 */
struct static_key { int enabled; };
struct static_key_false { struct static_key key; };
extern struct static_key_false key_a, key_b, key_c;

static inline __attribute__((always_inline)) _Bool arch_static_branch(struct static_key *key, _Bool branch)
{
	asm goto("1: nop\n\t"
		 ".pushsection __jump_table, \"aw\"\n\t.align 3\n\t"
		 ".long 1b - ., %l[l_yes] - .\n\t.quad %c0 - .\n\t.popsection"
		 : : "i"(&((char *)key)[branch]) : : l_yes);
	return 0;
l_yes:
	return 1;
}
#define static_branch_unlikely(x) __builtin_expect(arch_static_branch(&(x)->key, 0), 0)

struct vcpu { long r[16]; };
long note(long value);
long finish(const long *values);

long fault(struct vcpu *vcpu, long ipa, long hva, long slot, long write);

#ifndef REDUCTION_DRIVER
long fault(struct vcpu *vcpu, long ipa, long hva, long slot, long write)
{
	long v0 = vcpu->r[0], v1 = vcpu->r[1], v2 = vcpu->r[2], v3 = vcpu->r[3];
	long v4 = vcpu->r[4], v5 = vcpu->r[5], v6 = vcpu->r[6], v7 = vcpu->r[7];
	long v8 = vcpu->r[8], v9 = vcpu->r[9], v10 = vcpu->r[10], v11 = vcpu->r[11];
	long v12 = vcpu->r[12], v13 = vcpu->r[13], v14 = vcpu->r[14], v15 = vcpu->r[15];

	if (static_branch_unlikely(&key_a))
		v0 = note(v0);
	if (static_branch_unlikely(&key_b))
		v1 = note(v1);
	if (static_branch_unlikely(&key_c))
		v2 = note(v2);
	{
		long values[20] = { v0, v1, v2, v3, v4, v5, v6, v7, v8, v9, v10, v11, v12, v13,
				    v14, v15, ipa, hva, slot, write };
		return finish(values);
	}
}
#else
#include <stdio.h>
struct static_key_false key_a, key_b, key_c;
long note(long value) { return value ^ 1; }
long finish(const long *values)
{
	long sum = 0;
	for (int i = 0; i < 20; i++)
		sum = sum * 31 + values[i];
	return sum;
}
int main(void)
{
	static struct vcpu vcpus[16];
	unsigned long sum = 0;
	for (int i = 0; i < 16; i++)
		for (int j = 0; j < 16; j++)
			vcpus[i].r[j] = i * 16 + j;
	for (int round = 0; round < 100000; round++)
		sum += fault(&vcpus[round & 15], round, round * 3, round & 7, round & 1);
	printf("REDUCTION_CHECKSUM %lx\n", sum);
	return 0;
}
#endif
