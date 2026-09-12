/* Declared-base shift precision; storage precision remains the field width. */
#define CHECK(W) do { \
    struct U { unsigned long value : W; } u = { (1UL << W) - 1 }; \
    struct S { signed long value : W; } s = { -1 }; \
    unsigned long count = 1; \
    unsigned long expected = ((1UL << W) - 1) << count; \
    if ((u.value << count) != expected) return W; \
    if (((u.value) << 1UL) != expected) return W + 1; \
    if ((s.value << count) != -2L) return W + 2; \
    u.value = u.value << count; \
    if ((unsigned long)u.value != ((1UL << W) - 2)) return W + 3; \
} while (0)

struct msi_desc {
    unsigned long iommu_msi_iova : 58;
    unsigned long iommu_msi_shift : 6;
};

static unsigned long restore(struct msi_desc *desc) {
    return desc->iommu_msi_iova << desc->iommu_msi_shift;
}

int main(void) {
    CHECK(33);
    CHECK(40);
    CHECK(52);
    CHECK(58);
    struct msi_desc desc = { (1UL << 51) + 7, 12 };
    if (restore(&desc) != ((1UL << 63) + 28672)) return 1;
    struct { unsigned long v : 64; } full = { ~0UL };
    if ((full.v << 1) != ~1UL) return 2;
    return 0;
}
