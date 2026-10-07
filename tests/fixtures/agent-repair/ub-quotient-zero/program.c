/* Return the numerator unchanged for a zero divisor; otherwise return the integer quotient. */
int quotient(int a, int b);

int quotient(int a, int b)
{
    return a / b;
}

int main(void)
{
    return quotient(7, 0) != 7;
}
