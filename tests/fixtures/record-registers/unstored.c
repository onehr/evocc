/* A callee that reads a member that its caller never stored reads an
 * uninitialized object in every interpreter (EvoMIR 6.4, D29). */
struct p8 { char c; int i; };
static int take(struct p8 r) { return r.i + r.c; }
int main(void)
{
    struct p8 a;
    a.i = 2;
    return take(a);
}
