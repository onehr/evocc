int struct_difference(int first, int second)
{
    struct { int x; int y; } s;

    s.x = first;
    s.y = second;
    return s.y - s.x - 2;
}

int main(void)
{
    return struct_difference(3, 5);
}
