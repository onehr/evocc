/* Link-time addresses in integer objects are the same relocation as pointers. */
int object[4] = { 11, 22, 33, 44 };
static unsigned long base = (unsigned long)&object;
static unsigned long element = (unsigned long)&object[2];
static unsigned long byte_offset = (unsigned long)&object + 12;
struct descriptor { unsigned long data; };
static struct descriptor table[] = { { (unsigned long)&object[1] } };

int main(void)
{
    if (*(int *)byte_offset != 44 || *(int *)table[0].data != 22)
        return 3;
    return base != (unsigned long)&object ||
           element != (unsigned long)&object[2] ||
           *(int *)byte_offset != 44 || *(int *)table[0].data != 22;
}
