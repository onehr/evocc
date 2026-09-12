int object = 7;
static unsigned long address = (unsigned long)&object;
static int *round_trip = (int *)(unsigned long)&object;
int main(void)
{
    unsigned long copied;
    __builtin_memcpy(&copied, &round_trip, sizeof(copied));
    return *(int *)address != 7 || *(int *)copied != 7;
}
