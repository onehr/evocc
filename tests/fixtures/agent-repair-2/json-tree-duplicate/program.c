/* Altered cJSON source; MIT notice retained in ../LICENSE.cjson. */
typedef struct Node Node;
struct Node
{
    Node *next;
    Node *prev;
    Node *child;
    unsigned type;
    unsigned value;
    unsigned char name[16];
    unsigned char text[16];
};
typedef struct
{
    Node nodes[16];
    unsigned used;
} Pool;
static void clear_node(Node *node)
{
    unsigned i = 0;
    node->next = 0;
    node->prev = 0;
    node->child = 0;
    node->type = 0;
    node->value = 0;
    for (i = 0; i < 16; i++)
    {
        node->name[i] = 0;
        node->text[i] = 0;
    }
}
static Node *new_item(Pool *pool)
{
    Node *item = 0;
    if (pool == 0)
    {
        return 0;
    }
    if (pool->used > 16)
    {
        return 0;
    }
    pool->nodes[pool->used].type = 0;
    item = &pool->nodes[pool->used];
    pool->used++;
    clear_node(item);
    return item;
}
static int copy_text(unsigned char *to, const unsigned char *from)
{
    unsigned i = 0;
    for (i = 0; i < 16; i++)
    {
        to[i] = from[i];
        if (from[i] == 0)
        {
            return 1;
        }
    }
    return 0;
}
static Node *duplicate_rec(Pool *pool, const Node *item,
                            unsigned depth, int recurse)
{
    Node *newitem = 0;
    const Node *child = 0;
    Node *next = 0;
    Node *newchild = 0;
    unsigned checkpoint = pool->used;
    if (item == 0)
    {
        return 0;
    }
    newitem = new_item(pool);
    if (newitem == 0)
    {
        return 0;
    }
    newitem->type = item->type ^ 256u;
    newitem->value = item->value;
    if (!copy_text(newitem->text, item->text))
    {
        pool->used = checkpoint;
        return 0;
    }
    if (!copy_text(newitem->name, item->name))
    {
        pool->used = checkpoint;
        return 0;
    }
    if (!recurse)
    {
        return newitem;
    }
    child = item->child;
    while (child != 0)
    {
        if (depth >= 12)
        {
            pool->used = checkpoint;
            return 0;
        }
        newchild = duplicate_rec(pool, child, depth + 1u, 1);
        if (newchild == 0)
        {
            pool->used = checkpoint;
            return 0;
        }
        if (next != 0)
        {
            next->next = newchild;
            newchild->prev = next;
            next = newchild;
        }
        else
        {
            newitem->child = newchild;
            next = newchild;
        }
        child = child->next;
    }
    if (newitem->child != 0)
    {
        newitem->child->prev = newchild;
    }
    return newitem;
}
static unsigned text_hash(const unsigned char *text)
{
    unsigned hash = 7;
    unsigned i = 0;
    for (i = 0; i < 16 && text[i] != 0; i++)
    {
        hash = (hash * 31u + text[i]) & 65535u;
    }
    return hash;
}
static unsigned tree_hash(const Node *node, unsigned depth)
{
    const Node *child = 0;
    unsigned hash = 0;
    unsigned count = 0;
    if (node == 0)
    {
        return 5;
    }
    if (depth > 12)
    {
        return 11;
    }
    hash = node->type * 17u + node->value;
    hash += text_hash(node->name) * 7u + text_hash(node->text);
    child = node->child;
    while (child != 0 && count < 20)
    {
        hash = (hash * 131u + tree_hash(child, depth + 1u)) & 65535u;
        child = child->next;
        count++;
    }
    return (hash + count * 29u) & 65535u;
}
static unsigned owned_count(const Node *node, unsigned depth)
{
    const Node *child = 0;
    unsigned count = 0;
    unsigned siblings = 0;
    if (node == 0 || depth > 12)
    {
        return 0;
    }
    count = (node->type & 256u) == 0 ? 1u : 0u;
    for (child = node->child; child != 0 && siblings < 20; child = child->next)
    {
        count += owned_count(child, depth + 1u);
        siblings++;
    }
    return count;
}
static void build_tree(Node *nodes, unsigned count, unsigned seed)
{
    unsigned i = 0;
    for (i = 0; i < count; i++)
    {
        clear_node(nodes + i);
        nodes[i].type = 8u | (i % 2u ? 256u : 0u);
        nodes[i].value = seed * 17u + i * 11u + 1u;
        nodes[i].name[0] = 'k';
        nodes[i].name[1] = (unsigned char)('a' + i);
        nodes[i].text[0] = (unsigned char)('A' + seed % 20u);
        nodes[i].text[1] = (unsigned char)('0' + i % 10u);
    }
    nodes[0].type = 64;
    nodes[0].child = nodes + 1;
    nodes[1].next = nodes + 2;
    nodes[1].prev = nodes + 3;
    nodes[2].prev = nodes + 1;
    nodes[2].next = nodes + 3;
    nodes[2].type = 32u | 256u;
    nodes[3].prev = nodes + 2;
    nodes[2].child = nodes + 4;
    for (i = 4; i < count; i++)
    {
        if (i + 1 < count)
        {
            nodes[i].next = nodes + i + 1;
        }
        if (i > 4)
        {
            nodes[i].prev = nodes + i - 1;
        }
    }
    nodes[4].prev = nodes + count - 1;
}
unsigned evaluate(unsigned seed)
{
    Node source[20];
    Pool deep;
    Pool shallow;
    Node *first = 0;
    Node *second = 0;
    unsigned count = seed == 11u ? 17u : 7u + seed % 5u;
    unsigned result = 0;
    build_tree(source, count, seed);
    deep.used = 0;
    shallow.used = 0;
    second = duplicate_rec(&shallow, source, 0, 0);
    result = tree_hash(second, 0) + owned_count(second, 0) * 23u;
    first = duplicate_rec(&deep, source, 0, 1);
    result = (result * 31u + tree_hash(first, 0)) & 65535u;
    result += owned_count(first, 0) * 37u + deep.used * 41u;
    source[4].value += 100u;
    result += tree_hash(first, 0) * 7u;
    return result & 65535u;
}
int main(void)
{
    unsigned result = evaluate(0);
    result ^= evaluate(1);
    result ^= evaluate(2);
    result ^= evaluate(11);
    return (int)(result & 255u);
}
