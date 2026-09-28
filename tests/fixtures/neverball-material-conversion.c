/* Reduction of Neverball's unparenthesized tobyte macro argument.
 * Original spelling is expected to fail E3063, not acquire wrapping FP casts.
 */
typedef unsigned char GLubyte;
#ifdef ORIGINAL_MACRO
#define tobyte(f) ((GLubyte) (f * 255.0f))
#else
#define tobyte(f) ((GLubyte) ((f) * 255.0f))
#endif
#define toushort(f) ((int) (f) << 8 | tobyte((f) - (int) (f)))
int main(void)
{
    volatile float shininess = 40.0f;
    return toushort(shininess) != 10240;
}
