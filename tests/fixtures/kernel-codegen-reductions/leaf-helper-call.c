/* A trivial static helper stays an out-of-line, signed, framed call.
 *
 * Reduced from include/linux/page-flags.h const_folio_flags and its
 * folio_test_* users; EvoCC keeps 111 copies of const_folio_flags and 52 of
 * numa_node_id in the kernel. GCC inlines the helper to an address and a
 * bit test. EvoCC specializes the helper for n == 0 but keeps the call; the
 * callee has its own PAC sign/authenticate pair, frame record and unfolded
 * "mov #0; and; lsl; add" index arithmetic, and the caller keeps its live
 * values across each call. The hot path is three flag tests on one folio.
 * Measured (reduction-counts.py, 100,000 calls): EvoCC 55 in folio_state
 * plus 3 x 10 in const_folio_flags, GCC 9 instructions per call.
 */
struct page { unsigned long flags; unsigned long compound_head; unsigned long pad[6]; };
struct folio { struct page page; };

static const unsigned long *const_folio_flags(const struct folio *folio, unsigned n)
{
	const struct page *page = &folio->page;

	return &page[n].flags;
}

static inline __attribute__((always_inline)) int const_test_bit(unsigned long nr, const unsigned long *addr)
{
	return 1UL & (addr[nr / 64] >> (nr & 63));
}

static inline int folio_test_dirty(const struct folio *folio) { return const_test_bit(4, const_folio_flags(folio, 0)); }
static inline int folio_test_lru(const struct folio *folio) { return const_test_bit(5, const_folio_flags(folio, 0)); }
static inline int folio_test_head(const struct folio *folio) { return const_test_bit(6, const_folio_flags(folio, 0)); }

int folio_state(const struct folio *folio);

#ifndef REDUCTION_DRIVER
int folio_state(const struct folio *folio)
{
	return folio_test_dirty(folio) | folio_test_lru(folio) << 1 | folio_test_head(folio) << 2;
}
#else
#include <stdio.h>
int main(void)
{
	static struct folio folios[64];
	unsigned long sum = 0;
	for (int i = 0; i < 64; i++)
		folios[i].page.flags = (unsigned long)i << 3;
	for (int round = 0; round < 100000; round++)
		sum += folio_state(&folios[round & 63]);
	printf("REDUCTION_CHECKSUM %lx\n", sum);
	return 0;
}
#endif
