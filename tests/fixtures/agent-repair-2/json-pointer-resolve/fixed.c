/* Altered cJSON source; MIT notice retained in ../LICENSE.cjson. */
/* Missing keys/array members, including paths continuing after a missing
 * value, return null. Lookup uses caller-owned nodes.
 * Only evaluate seeds 0..11 are supported.
 */
typedef struct Node Node;
struct Node
{
    Node *next;
    Node *child;
    const unsigned char *key;
    unsigned type;
    unsigned number;
};
static unsigned lower_byte(unsigned byte)
{
    if (byte >= 'A' && byte <= 'Z')
    {
        return byte + ('a' - 'A');
    }
    return byte;
}
static int compare_pointers(const unsigned char *name,
                            const unsigned char *pointer, int sensitive)
{
    if (name == 0 || pointer == 0)
    {
        return 0;
    }
    for (; *name != 0 && *pointer != 0 && *pointer != '/'; name++, pointer++)
    {
        if (*pointer == '~')
        {
            if ((pointer[1] != '0' || *name != '~') &&
                (pointer[1] != '1' || *name != '/'))
            {
                return 0;
            }
            pointer++;
        }
        else if ((!sensitive && lower_byte(*name) != lower_byte(*pointer)) ||
                 (sensitive && *name != *pointer))
        {
            return 0;
        }
    }
    if (((*pointer != 0 && *pointer != '/') != (*name != 0)))
    {
        return 0;
    }
    return 1;
}
static Node *get_array_item(Node *array, unsigned item)
{
    Node *child = array ? array->child : 0;
    while (child != 0 && item > 0)
    {
        item--;
        child = child->next;
    }
    return child;
}
static int decode_array_index(const unsigned char *pointer, unsigned *index)
{
    unsigned parsed = 0;
    unsigned position = 0;
    if (pointer[0] == '0' && pointer[1] != 0 && pointer[1] != '/')
    {
        return 0;
    }
    if (pointer[0] < '0' || pointer[0] > '9')
    {
        return 0;
    }
    for (; pointer[position] >= '0' && pointer[position] <= '9'; position++)
    {
        unsigned digit = (unsigned)pointer[position] - '0';
        if (parsed > (65535u - digit) / 10u)
        {
            return 0;
        }
        parsed = 10u * parsed + digit;
    }
    if (pointer[position] != 0 && pointer[position] != '/')
    {
        return 0;
    }
    *index = parsed;
    return 1;
}
static Node *get_item_pointer(Node *object, const unsigned char *pointer,
                              int sensitive)
{
    Node *current = object;
    if (pointer == 0)
    {
        return 0;
    }
    while (pointer[0] == '/' && current != 0)
    {
        pointer++;
        if (current->type == 2)
        {
            unsigned index = 0;
            if (!decode_array_index(pointer, &index))
            {
                return 0;
            }
            current = get_array_item(current, index);
        }
        else if (current->type == 3)
        {
            current = current->child;
            while (current != 0 && !compare_pointers(current->key, pointer, sensitive))
            {
                current = current->next;
            }
        }
        else
        {
            return 0;
        }
        while (pointer[0] != 0 && pointer[0] != '/')
        {
            pointer++;
        }
    }
    return current;
}
static void reset_node(Node *node, unsigned type, unsigned value)
{
    node->next = 0;
    node->child = 0;
    node->key = 0;
    node->type = type;
    node->number = value;
}
static unsigned value_from_pointer(Node *object, const unsigned char *pointer,
                                    int sensitive)
{
    Node *item = get_item_pointer(object, pointer, sensitive);
    if (item == 0)
    {
        return 7;
    }
    return item->number * 13u + item->type;
}
static unsigned make_index_pointer(unsigned char *output, unsigned index)
{
    unsigned position = 6;
    output[0] = '/';
    output[1] = 'L';
    output[2] = 'i';
    output[3] = 's';
    output[4] = 't';
    output[5] = '/';
    if (index >= 10u)
    {
        output[position++] = (unsigned char)('0' + index / 10u);
    }
    output[position++] = (unsigned char)('0' + index % 10u);
    output[position] = 0;
    return position;
}
static unsigned list_score(Node *node)
{
    unsigned hash = 0;
    unsigned count = 0;
    while (node != 0 && count < 16)
    {
        hash = (hash * 31u + node->number) & 65535u;
        node = node->next;
        count++;
    }
    return (hash + count * 17u) & 65535u;
}
unsigned evaluate(unsigned seed)
{
    Node root;
    Node array;
    Node slash;
    Node values[12];
    unsigned char query[16];
    unsigned i = 0;
    unsigned result = 0;
    reset_node(&root, 3, 1);
    reset_node(&array, 2, 2);
    reset_node(&slash, 1, seed * 19u + 3u);
    root.child = &array;
    array.key = (const unsigned char *)"List";
    array.next = &slash;
    slash.key = (const unsigned char *)"a/b";
    array.child = values;
    for (i = 0; i < 12; i++)
    {
        reset_node(values + i, 1, (seed + 1u) * 11u + i * 7u);
        if (i + 1 < 12)
        {
            values[i].next = values + i + 1;
        }
    }
    result = make_index_pointer(query, seed % 12u);
    result += value_from_pointer(&root, query, 1);
    result += value_from_pointer(&root, (const unsigned char *)"/A~1B", 0) * 17u;
    result += value_from_pointer(&root, (const unsigned char *)"/a~1b", 1) * 23u;
    result += value_from_pointer(&root, (const unsigned char *)"/List/01", 1);
    result += list_score(array.child);
    result += value_from_pointer(&root, seed == 11u ?
              (const unsigned char *)"/List/30/value" : query, 1);
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
