struct info { int code; };
static int classify(struct info *p)
{
    switch ((unsigned long)p) {
    case (unsigned long)(struct info *)0: return 3;
    case (unsigned long)(struct info *)1: return 5;
    default: return 7;
    }
}
int main(void)
{
    return classify((struct info *)0) != 3 || classify((struct info *)1) != 5
        || classify((struct info *)2) != 7;
}
