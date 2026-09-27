/* A command projector must not erase -fpack-struct=1 and publish this as green.
 * Reference packed layout: 5 bytes; ordinary target layout: 8 bytes.
 * This fixture also compiles without assertions, making runtime bits decisive.
 */
struct record {
    char tag;
    int value;
};

int main(void)
{
    return sizeof(struct record) == 5 ? 0 : 1;
}
