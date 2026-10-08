/* Altered cJSON source; MIT notice retained in ../LICENSE.cjson. */
/* Identical nodes with supported types compare equal; otherwise missing text
 * in a string node makes the comparison unequal. Null nodes compare unequal.
 * Only evaluate seeds 0..11 are supported.
 */
typedef struct Node Node;
struct Node
{
    Node *next;
    Node *child;
    const unsigned char *name;
    const unsigned char *text;
    unsigned type;
    int value;
};
static unsigned lower_byte(unsigned byte)
{
    if (byte >= 'A' && byte <= 'Z')
    {
        return byte + ('a' - 'A');
    }
    return byte;
}
static int compare_text(const unsigned char *a, const unsigned char *b,
                        int sensitive)
{
    while (*a != 0 && *b != 0)
    {
        unsigned left = sensitive ? *a : lower_byte(*a);
        unsigned right = sensitive ? *b : lower_byte(*b);
        if (left != right)
        {
            return 0;
        }
        a++;
        b++;
    }
    return *a == *b;
}
static const Node *get_object_item(const Node *object,
                                   const unsigned char *name, int sensitive)
{
    const Node *child = 0;
    if (object == 0 || name == 0)
    {
        return 0;
    }
    for (child = object->child; child != 0; child = child->next)
    {
        if (child->name != 0 && compare_text(child->name, name, sensitive))
        {
            return child;
        }
    }
    return 0;
}
static int compare_json(const Node *a, const Node *b, int sensitive)
{
    unsigned type = 0;
    if (a == 0 || b == 0 || (a->type & 255u) != (b->type & 255u))
    {
        return 0;
    }
    type = a->type & 255u;
    switch (type)
    {
        case 1:
        case 2:
        case 4:
        case 8:
        case 16:
        case 32:
        case 64:
            break;
        default:
            return 0;
    }
    if (a == b)
    {
        return 1;
    }
    switch (type)
    {
        case 1:
        case 2:
        case 4:
            return 1;
        case 8:
            return a->value == b->value;
        case 16:
            if (b->text == 0)
            {
                return 0;
            }
            return compare_text(a->text, b->text, 1);
        case 32:
        {
            const Node *a_element = a->child;
            const Node *b_element = b->child;
            while (a_element != 0 && b_element != 0)
            {
                if (!compare_json(a_element, b_element, sensitive))
                {
                    return 0;
                }
                a_element = a_element->next;
                b_element = b_element->next;
            }
            if (a_element != b_element)
            {
                return 0;
            }
            return 1;
        }
        case 64:
        {
            const Node *a_element = 0;
            const Node *b_element = 0;
            for (a_element = a->child; a_element != 0; a_element = a_element->next)
            {
                b_element = get_object_item(b, a_element->name, sensitive);
                if (b_element == 0)
                {
                    return 0;
                }
                if (!compare_json(a_element, b_element, sensitive))
                {
                    return 0;
                }
            }
            for (b_element = a->child; b_element != 0; b_element = b_element->next)
            {
                a_element = get_object_item(a, b_element->name, sensitive);
                if (a_element == 0)
                {
                    return 0;
                }
                if (!compare_json(b_element, a_element, sensitive))
                {
                    return 0;
                }
            }
            return 1;
        }
        default:
            return 0;
    }
}
static void initialize_node(Node *node, unsigned type, int value,
                            const unsigned char *name,
                            const unsigned char *text)
{
    node->next = 0;
    node->child = 0;
    node->name = name;
    node->text = text;
    node->type = type;
    node->value = value;
}
static void make_members(Node *nodes, unsigned seed, int upper)
{
    const unsigned char *names[3] = {
        (const unsigned char *)"alpha",
        (const unsigned char *)"beta",
        (const unsigned char *)"extra"
    };
    const unsigned char *caps[3] = {
        (const unsigned char *)"ALPHA",
        (const unsigned char *)"BETA",
        (const unsigned char *)"EXTRA"
    };
    unsigned i = 0;
    for (i = 0; i < 3; i++)
    {
        initialize_node(nodes + i, 8, (int)(seed * 17u + i * 7u),
                        upper ? caps[i] : names[i], 0);
        if (i + 1 < 3)
        {
            nodes[i].next = nodes + i + 1;
        }
    }
}
static unsigned compare_pair(const Node *a, const Node *b, int sensitive)
{
    unsigned forward = (unsigned)compare_json(a, b, sensitive);
    unsigned reverse = (unsigned)compare_json(b, a, sensitive);
    unsigned identity = (unsigned)compare_json(a, a, sensitive);
    return forward * 17u + reverse * 31u + identity * 43u;
}
unsigned evaluate(unsigned seed)
{
    Node a;
    Node b;
    Node members_a[3];
    Node members_b[3];
    Node array_a;
    Node array_b;
    Node text_a;
    Node text_b;
    unsigned result = 0;
    int sensitive = (int)(seed % 2u);
    initialize_node(&a, 64, 0, 0, 0);
    initialize_node(&b, 64u | 256u, 0, 0, 0);
    make_members(members_a, seed, 0);
    make_members(members_b, seed, (int)(seed % 3u == 1u));
    members_a[1].next = 0;
    if (seed % 3u == 0)
    {
        members_b[1].next = 0;
    }
    if (seed % 4u == 2u)
    {
        members_b[1].value++;
    }
    a.child = members_a;
    b.child = members_b;
    result = compare_pair(&a, &b, sensitive);
    initialize_node(&array_a, 32, 0, 0, 0);
    initialize_node(&array_b, 32, 0, 0, 0);
    array_a.child = &a;
    array_b.child = &b;
    result = result * 13u + compare_pair(&array_a, &array_b, sensitive);
    initialize_node(&text_a, 16, 0, 0, (const unsigned char *)"sample");
    initialize_node(&text_b, 16, 0, 0, (const unsigned char *)"sample");
    result += compare_pair(&text_a, &text_b, sensitive) * 19u;
    if (seed == 11u)
    {
        text_a.text = 0;
    }
    result += (unsigned)compare_json(&text_a, &text_b, sensitive) * 29u;
    result += seed * 37u;
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
