int goto_route(int input)
{
    goto next;
success:
    return input;
next:
    goto success;
}

int main(void)
{
    return goto_route(7);
}
