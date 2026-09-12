struct operation { int value; };
struct parameters { long header; int data[4]; };
static const struct operation *disabled = &(*(const struct operation *)0);
static void *first = &((struct parameters *)0)->data[0];
static void *last = &((struct parameters *)0)->data[3];
static void *based = &((struct parameters *)0x1000)->data[2];
int main(void)
{
    return disabled != 0 || (unsigned long)first != 8 ||
           (unsigned long)last != 20 || (unsigned long)based != 0x1010;
}
