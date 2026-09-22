/* TinyCC's fallback offsetof, converted into a narrow static table field. */
typedef unsigned long size_t;
struct state { char first; long payload; unsigned char flag; };
struct option { unsigned short offset; unsigned short flags; };
struct large { unsigned char padding[264]; unsigned char field; };
static const struct option options[] = {
    { (size_t)&((struct state *)0)->flag, 1 },
    { (size_t)&((struct state *)0)->payload, 2 },
};
static const unsigned long narrowed =
    (unsigned char)(size_t)&((struct large *)0)->field;
int main(void)
{
    return options[0].offset != 16 || options[1].offset != 8 || narrowed != 8;
}
