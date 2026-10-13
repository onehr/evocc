/* A one-word struct value is copied through stack slots at each use.
 *
 * Reduced from mm/pgtable-generic.c ___pte_offset_map (5.9x GCC per fault,
 * called five times per demand fault). pmd_t and pte_t wrap one u64. GCC
 * keeps pmdval in a register. EvoCC keeps every pmd_t temporary in its own
 * stack slot: it zero-fills those slots for -ftrivial-auto-var-init=zero,
 * copies the value slot to slot before each by-value helper call, and adds
 * a stack-protector canary because &pmdval is taken. Each helper result is
 * set to 0 or 1, masked, widened and compared again. The hot path is a
 * present, non-huge, good PMD.
 * Measured (reduction-counts.py, 100,000 calls): EvoCC 115 and GCC 23
 * instructions per call.
 */
#define __scalar_type_to_expr_cases(type) unsigned type: (unsigned type)0, signed type: (signed type)0
#define __unqual_scalar_typeof(x) typeof(_Generic((x), char: (char)0, __scalar_type_to_expr_cases(char), \
	__scalar_type_to_expr_cases(short), __scalar_type_to_expr_cases(int), \
	__scalar_type_to_expr_cases(long), __scalar_type_to_expr_cases(long long), default: (x)))
#define READ_ONCE(x) (*(const volatile __unqual_scalar_typeof(x) *)&(x))

typedef struct { unsigned long pmd; } pmd_t;
typedef struct { unsigned long pte; } pte_t;

static inline pmd_t pmdp_get_lockless(pmd_t *pmdp) { return READ_ONCE(*pmdp); }
static inline unsigned long pmd_val(pmd_t pmd) { return pmd.pmd; }
static inline int pmd_none(pmd_t pmd) { return !pmd_val(pmd); }
static inline int pmd_trans_huge(pmd_t pmd) { return pmd_val(pmd) && !(pmd_val(pmd) & 2); }
static inline int pmd_bad(pmd_t pmd) { return !(pmd_val(pmd) & 2); }
static inline pte_t *pte_map(pmd_t *pmd, unsigned long addr)
{
	return (pte_t *)((pmd_val(*pmd) & 0xfffffffff000UL) | 0xffff000000000000UL) + ((addr >> 12) & 511);
}
void rcu_read_lock(void);
void rcu_read_unlock(void);
void pmd_clear_bad(pmd_t *pmd);

pte_t *pte_offset_map(pmd_t *pmd, unsigned long addr, pmd_t *pmdvalp);

#ifndef REDUCTION_DRIVER
pte_t *pte_offset_map(pmd_t *pmd, unsigned long addr, pmd_t *pmdvalp)
{
	pmd_t pmdval;

	rcu_read_lock();
	pmdval = pmdp_get_lockless(pmd);
	if (pmdvalp)
		*pmdvalp = pmdval;
	if (__builtin_expect(pmd_none(pmdval), 0))
		goto nomap;
	if (__builtin_expect(pmd_trans_huge(pmdval), 0))
		goto nomap;
	if (__builtin_expect(pmd_bad(pmdval), 0)) {
		pmd_clear_bad(pmd);
		goto nomap;
	}
	return pte_map(&pmdval, addr);
nomap:
	rcu_read_unlock();
	return 0;
}
#else
#include <stdio.h>
static int depth;
void rcu_read_lock(void) { depth++; }
void rcu_read_unlock(void) { depth--; }
void pmd_clear_bad(pmd_t *pmd) { pmd->pmd = 0; }
int main(void)
{
	static pmd_t pmds[16];
	pmd_t seen;
	unsigned long sum = 0;
	for (int i = 0; i < 16; i++)
		pmds[i].pmd = 0x40000000UL * (i + 1) | 3;
	for (int round = 0; round < 100000; round++)
		sum += (unsigned long)pte_offset_map(&pmds[round & 15], (unsigned long)round << 12, &seen) + seen.pmd;
	printf("REDUCTION_CHECKSUM %lx %d\n", sum, depth);
	return 0;
}
#endif
