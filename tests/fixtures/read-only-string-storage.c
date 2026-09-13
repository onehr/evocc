/* Link with __readonly_start/end bracketing the complete .rodata section. */
extern char __readonly_start[], __readonly_end[];
static char writable[] = "none";
static int blank;

const char *literal(void) { return "none"; }
const char *empty(void) { return ""; }
const unsigned int *wide_empty(void) { return U""; }
const char *embedded_zero(void) { return "a\0b"; }

static int in_rodata(const void *pointer)
{
    unsigned long address = (unsigned long)pointer;
    return address >= (unsigned long)__readonly_start &&
           address < (unsigned long)__readonly_end;
}

int main(void)
{
    if (!in_rodata(literal()) || literal()[0] != 'n' || literal()[4] != 0)
        return 1;
    if (!in_rodata(empty()) || empty()[0] != 0)
        return 2;
    if (!in_rodata(wide_empty()) || wide_empty()[0] != 0)
        return 3;
    if (!in_rodata(embedded_zero()) || embedded_zero()[2] != 'b')
        return 4;
    if (in_rodata(writable) || in_rodata(&blank))
        return 5;
    writable[0] = 'N';
    blank = 7;
    return writable[0] != 'N' || blank != 7;
}
