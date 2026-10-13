/* Loop-carried values in EvoCC pass through stack slots on every edge.
 *
 * Reduced from mm/page_alloc.c __free_pages_ok -> free_pages_prepare, the top
 * boot-time excess (26.7x GCC). Six arguments live only after the loop. GCC
 * keeps them in callee-saved registers once. EvoCC carries all of them as
 * block arguments through each loop block: it copies spilled values to their
 * own slots (ldr/str of one address), shuffles them through registers, and
 * re-materializes the static-branch result. The loop also re-checks the
 * shift count of the loop-invariant 1 << order on every iteration.
 * The hot path is order 10, no compound page, static key disabled.
 * Measured (reduction-counts.py, 100 calls x 1,023 iterations): EvoCC
 * 179,135 and GCC 8,236 instructions per call, about 175 and 8 per iteration.
 */
struct static_key { int enabled; };
struct static_key_false { struct static_key key; };
extern struct static_key_false check_key;

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

struct page { unsigned long flags; unsigned long pad[7]; };
int tail_prepare(struct page *head, struct page *p);
int page_is_bad(struct page *p);
void finish(struct page *page, unsigned int order, unsigned long pfn, int mt, int init,
	    int poison, int compound, unsigned long fpi, void *zone, int bad);

void free_ok(struct page *page, unsigned int order, unsigned long fpi, void *zone,
	     int mt, int init, int poison);

#ifndef REDUCTION_DRIVER
void free_ok(struct page *page, unsigned int order, unsigned long fpi, void *zone,
	     int mt, int init, int poison)
{
	unsigned long pfn = (unsigned long)page >> 6;
	int compound = page->flags & 1;
	int bad = 0;

	for (int i = 1; i < (1 << order); i++) {
		if (compound)
			bad += tail_prepare(page, page + i);
		if (static_branch_unlikely(&check_key)) {
			if (page_is_bad(page + i)) {
				bad++;
				continue;
			}
		}
		(page + i)->flags &= ~0x1fffUL;
	}
	finish(page, order, pfn, mt, init, poison, compound, fpi, zone, bad);
}
#else
#include <stdio.h>
struct static_key_false check_key;
static struct page pages[1024];
static unsigned long sum;
int tail_prepare(struct page *head, struct page *p) { return p != head; }
int page_is_bad(struct page *p) { return p->flags == 1; }
void finish(struct page *page, unsigned int order, unsigned long pfn, int mt, int init,
	    int poison, int compound, unsigned long fpi, void *zone, int bad)
{
	sum += (unsigned long)(page - pages) + order + (pfn & 0) + mt + init + poison + compound
	       + fpi + (zone != 0) + bad;
}
int main(void)
{
	for (int round = 0; round < 100; round++) {
		for (int i = 0; i < 1024; i++)
			pages[i].flags = 0xffffUL << 1;
		free_ok(pages, 10, 3, pages, 1, 0, 0);
		for (int i = 0; i < 1024; i++)
			sum += pages[i].flags;
	}
	printf("REDUCTION_CHECKSUM %lx\n", sum);
	return 0;
}
#endif
