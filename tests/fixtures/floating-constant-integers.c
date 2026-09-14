struct rate { int bitrate; };
static struct rate rates[] = {{5.5 * 10}, {16 * 53.00}, {16 * 803.00}};
static int negative = -5.9;
static unsigned int fraction = 0.9;
static int comparison = 1.5 < 2.0;
int main(void)
{
    unsigned int frequency = (unsigned int)(10.7 * 16000);
    int rounded = (int)((16777216.0f + 1.0f) - 16777216.0f);
    int precise = (int)((16777216.0 + 1.0) - 16777216.0);
    int selected = (int)(1 ? 16777217 : 0.0f);
    return !(rates[0].bitrate == 55 && rates[1].bitrate == 848
        && rates[2].bitrate == 12848 && negative == -5 && fraction == 0
        && comparison == 1 && frequency == 171200 && rounded == 0
        && precise == 1 && selected == 16777216);
}
