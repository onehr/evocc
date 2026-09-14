typedef union { int **pages; long **other; } pages_arg
    __attribute__((transparent_union));

#ifdef CALLEE
int consume(pages_arg arg, int i) { return arg.pages ? *arg.pages[i] : 0; }
#else
extern int consume(pages_arg, int);
int main(void)
{
    int a = 17, b = 29;
    int *pages[] = { &a, &b };
    int (*fp)(pages_arg, int) = consume;
    if (consume(pages, 1) != 29) return 1;
    if (fp(pages, 0) != 17) return 2;
    return consume(0, 0) != 0;
}
#endif
