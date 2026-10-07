/* Return the indexed member of {3, 5, 7, 11}; return zero for an index outside 0..3. */
int lookup(int index);

int lookup(int index)
{
    int values[4] = {3, 5, 7, 11};
    if (index < 0 || index >= 4) return 0;
    return values[index];
}

int main(void)
{
    return lookup(4) != 0;
}
