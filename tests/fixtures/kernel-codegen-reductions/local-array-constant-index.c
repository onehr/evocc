/* A small local array stays in memory and each constant index is rebuilt.
 *
 * Reduced from crypto/sha3_generic.c keccakf_round (theta step), the top
 * single boot-time excess: EvoCC keccakf executes 259 M instructions where
 * GCC's inlined rounds execute about 50 M. GCC keeps bc[] in registers.
 * EvoCC zero-fills bc[] for -ftrivial-auto-var-init=zero although every
 * element is written before use, stores each element to the stack, computes
 * each constant element address as "mov #k; sxtw; lsl #3; add", then reloads
 * the elements. The hot path is one theta step on a 25-word state.
 * Measured (reduction-counts.py, 100,000 calls): EvoCC 329 and GCC 94
 * instructions per call.
 */
typedef unsigned long long u64;

static inline u64 rol64(u64 word, unsigned int shift)
{
	return (word << (shift & 63)) | (word >> ((-shift) & 63));
}

void theta(u64 st[25]);

#ifndef REDUCTION_DRIVER
void theta(u64 st[25])
{
	u64 bc[5], t;

	bc[0] = st[0] ^ st[5] ^ st[10] ^ st[15] ^ st[20];
	bc[1] = st[1] ^ st[6] ^ st[11] ^ st[16] ^ st[21];
	bc[2] = st[2] ^ st[7] ^ st[12] ^ st[17] ^ st[22];
	bc[3] = st[3] ^ st[8] ^ st[13] ^ st[18] ^ st[23];
	bc[4] = st[4] ^ st[9] ^ st[14] ^ st[19] ^ st[24];

	t = bc[4] ^ rol64(bc[1], 1);
	st[0] ^= t; st[5] ^= t; st[10] ^= t; st[15] ^= t; st[20] ^= t;
	t = bc[0] ^ rol64(bc[2], 1);
	st[1] ^= t; st[6] ^= t; st[11] ^= t; st[16] ^= t; st[21] ^= t;
	t = bc[1] ^ rol64(bc[3], 1);
	st[2] ^= t; st[7] ^= t; st[12] ^= t; st[17] ^= t; st[22] ^= t;
	t = bc[2] ^ rol64(bc[4], 1);
	st[3] ^= t; st[8] ^= t; st[13] ^= t; st[18] ^= t; st[23] ^= t;
	t = bc[3] ^ rol64(bc[0], 1);
	st[4] ^= t; st[9] ^= t; st[14] ^= t; st[19] ^= t; st[24] ^= t;
}
#else
#include <stdio.h>
int main(void)
{
	static u64 st[25];
	for (int i = 0; i < 25; i++)
		st[i] = 0x9e3779b97f4a7c15ULL * (i + 1);
	for (int round = 0; round < 100000; round++)
		theta(st);
	u64 sum = 0;
	for (int i = 0; i < 25; i++)
		sum ^= st[i] + i;
	printf("REDUCTION_CHECKSUM %llx\n", sum);
	return 0;
}
#endif
