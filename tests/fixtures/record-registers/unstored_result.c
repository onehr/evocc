/* A result member that the callee never stored is uninitialized in the
 * caller (EvoMIR 6.4, D29). */
struct b3 { char a, b, c; };
static struct b3 make(char a) { struct b3 r; r.a = a; return r; }
int main(void)
{
    struct b3 r = make(1);
    return r.b;
}
