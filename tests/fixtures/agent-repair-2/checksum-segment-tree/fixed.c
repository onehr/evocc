/* Written for this corpus; see origin.md for the algorithm contract. */
/* Segment tree over 0..64 byte values (0..255). Hashes are the ordered fold
 * h = h * 2 + byte modulo 2^32; concatenation preserves byte order.
 * Queries use half-open bounds 0 <= first <= last <= count; empty ranges and
 * invalid bounds return the empty segment. Updates need an existing index
 * and a byte value. Only evaluate seeds 0..11 are supported.
 */
typedef struct {
    unsigned hash;
    unsigned length;
} Segment;
typedef struct {
    Segment nodes[128];
    unsigned count;
} ChecksumTree;
static Segment empty_segment(void)
{
    Segment result;
    result.hash = 0;
    result.length = 0;
    return result;
}
static unsigned power_two(unsigned exponent)
{
    unsigned result = 1;
    unsigned base = 2;
    if (exponent < 32)
        return 1U << exponent;
    while (exponent != 0) {
        if (exponent & 1)
            result *= base;
        exponent >>= 1;
        base *= base;
    }
    return result;
}
static Segment join(Segment left, Segment right)
{
    Segment result;
    unsigned factor = power_two(right.length);
    result.hash = left.hash * factor + right.hash;
    result.length = left.length + right.length;
    return result;
}
static void build_node(ChecksumTree *tree, const unsigned char *values,
                       unsigned node, unsigned begin, unsigned end)
{
    unsigned middle;
    if (end - begin == 1) {
        tree->nodes[node].hash = values[begin];
        tree->nodes[node].length = 1;
        return;
    }
    middle = begin + (end - begin) / 2;
    build_node(tree, values, node * 2 + 1, begin, middle);
    build_node(tree, values, node * 2 + 2, middle, end);
    tree->nodes[node] = join(tree->nodes[node * 2 + 1], tree->nodes[node * 2 + 2]);
}
static int tree_build(ChecksumTree *tree, const unsigned char *values, unsigned count)
{
    unsigned i;
    if (count > 64)
        return 0;
    tree->count = count;
    for (i = 0; i < 128; ++i)
        tree->nodes[i] = empty_segment();
    if (count != 0)
        build_node(tree, values, 0, 0, count);
    return 1;
}
static void update_node(ChecksumTree *tree, unsigned node, unsigned begin,
                        unsigned end, unsigned item, unsigned value)
{
    unsigned middle;
    if (end - begin == 1) {
        tree->nodes[node].hash = value;
        return;
    }
    middle = begin + (end - begin) / 2;
    if (item < middle)
        update_node(tree, node * 2 + 1, begin, middle, item, value);
    else
        update_node(tree, node * 2 + 2, middle, end, item, value);
    tree->nodes[node] = join(tree->nodes[node * 2 + 1], tree->nodes[node * 2 + 2]);
}
static int tree_update(ChecksumTree *tree, unsigned item, unsigned value)
{
    if (item >= tree->count || value > 255)
        return 0;
    update_node(tree, 0, 0, tree->count, item, value);
    return 1;
}
static Segment query_node(const ChecksumTree *tree, unsigned node, unsigned begin,
                           unsigned end, unsigned first, unsigned last)
{
    unsigned middle;
    Segment left;
    Segment right;
    if (last <= begin || end <= first)
        return empty_segment();
    if (first <= begin && end <= last)
        return tree->nodes[node];
    middle = begin + (end - begin) / 2;
    left = query_node(tree, node * 2 + 1, begin, middle, first, last);
    right = query_node(tree, node * 2 + 2, middle, end, first, last);
    return join(left, right);
}
static Segment tree_query(const ChecksumTree *tree, unsigned first, unsigned last)
{
    if (first > last || last > tree->count || first == last)
        return empty_segment();
    return query_node(tree, 0, 0, tree->count, first, last);
}
static Segment direct_query(const unsigned char *values, unsigned first, unsigned last)
{
    Segment result = empty_segment();
    unsigned i;
    for (i = first; i < last; ++i) {
        result.hash = result.hash * 2 + values[i];
        ++result.length;
    }
    return result;
}
static unsigned audit_ranges(const ChecksumTree *tree, const unsigned char *values)
{
    unsigned first;
    unsigned last;
    unsigned mismatches = 0;
    for (first = 0; first <= tree->count; ++first) {
        for (last = first; last <= tree->count; ++last) {
            Segment actual = tree_query(tree, first, last);
            Segment expected = direct_query(values, first, last);
            if (actual.hash != expected.hash || actual.length != expected.length)
                ++mismatches;
        }
    }
    return mismatches;
}
unsigned long long evaluate(unsigned seed)
{
    ChecksumTree tree;
    unsigned char values[64];
    unsigned count = seed == 11 ? 64 : 8 + seed;
    unsigned i;
    unsigned long long digest = 0;
    for (i = 0; i < count; ++i)
        values[i] = (unsigned char)(((i * 13 + seed * 7) ^ (i >> 1)) & 1);
    if (seed == 11)
        values[0] = 1;
    tree_build(&tree, values, count);
    for (i = 0; i < 3; ++i) {
        unsigned position = (seed + i * 5) % count;
        values[position] ^= 1;
        tree_update(&tree, position, values[position]);
        digest = digest * 65599 + tree.nodes[0].hash;
    }
    for (i = 0; i < 7; ++i) {
        unsigned first = (seed + i * 3) % count;
        Segment part = tree_query(&tree, first, count);
        digest = digest * 33 ^ part.hash ^ ((unsigned long long)part.length << 32);
    }
    return digest ^ audit_ranges(&tree, values);
}
int main(void)
{
    Segment left = {17, 5};
    Segment short_right = {3, 2};
    Segment whole_word = {29, 32};
    Segment a = join(left, short_right);
    Segment b = join(a, short_right);
    Segment c = join(b, whole_word);
    return c.hash != whole_word.hash || c.length != 41;
}
