typedef unsigned long word;
enum state { STATE_EMPTY = -1, STATE_READY = 7 };
struct node {
    struct node *next;
    const word *values;
    unsigned count : 5;
    signed mode : 3;
    union { word value; unsigned char bytes[8]; } data;
    enum state state;
    int tail[];
};
struct node root;
struct __attribute__((packed)) packet { char c; unsigned short bits : 9; int n; };
struct packet packet;
typedef int (*callback)(const struct node *, unsigned);
callback callback_slot;

int inspect_node(const struct node *node, unsigned index)
{
    return node->state + node->count + node->mode + (int)node->values[index];
}

int call_callback(callback function, const struct node *node)
{
    return function(node, 0);
}
