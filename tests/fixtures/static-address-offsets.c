/* Absolute address constants keep pointer scaling and integer-cast width. */
static void *timer_entry = (void *)0x300 + 0x1000UL;
static int *words = (int *)0x1000 + 2;
static short *halves = 2 + (short *)0x1000;
static char *previous = (char *)0x1000 - 3;
static char *negative = (char *)0x1000 + -3;
static void *narrowed = (void *)(unsigned short)0x10003UL + 4;

int main(void)
{
    return (unsigned long)timer_entry != 0x1300UL ||
           (unsigned long)words != 0x1008UL ||
           (unsigned long)halves != 0x1004UL ||
           (unsigned long)previous != 0xffdUL ||
           (unsigned long)negative != 0xffdUL ||
           (unsigned long)narrowed != 7;
}
