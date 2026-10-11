/* EqSat removes the addition for every unsigned input, not just these checks. */
unsigned int add_zero(unsigned int x)
{
    return x + 0u;
}

int main(void)
{
    return add_zero(0u) != 0u
        || add_zero(42u) != 42u
        || add_zero(4294967295u) != 4294967295u;
}
