extern void invalid_layout(void);
struct header {
    unsigned char tag;
    unsigned char size;
    unsigned char low : 7, enabled : 1;
    unsigned char tail;
    unsigned int word;
};
int main(void)
{
    if (((unsigned char *)&(struct header){ .tag = 3 })[0] != 3)
        invalid_layout();
    if (((unsigned char *)&(struct header){ .enabled = 1 })[2] != 128)
        invalid_layout();
    if (((unsigned int *)&(struct header){ .word = 0xdeadbeef })[1] != 0xdeadbeef)
        invalid_layout();
    if (((unsigned char *)&(struct header){ .tag = 3 })[1] != 0)
        invalid_layout();
    return 0;
}
