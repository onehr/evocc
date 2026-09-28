/* Deterministic doomgeneric platform adapter, not a replacement game engine.
 * stdout carries complete 320x200 BGRA frames; engine messages go to stderr.
 * No wall clock, graphics server, sound, or host keyboard affects this lane.
 */
#include "doomgeneric.h"
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>

static FILE *frames;
static unsigned frame_count;
static uint32_t clock_ms = 1;

void DG_Init(void) {}
void DG_SetWindowTitle(const char *title) { (void)title; }
void DG_SleepMs(uint32_t ms) { clock_ms += ms; }
uint32_t DG_GetTicksMs(void) { return clock_ms++; }
int DG_GetKey(int *pressed, unsigned char *key)
{
    (void)pressed;
    (void)key;
    return 0;
}

void DG_DrawFrame(void)
{
    size_t count = DOOMGENERIC_RESX * DOOMGENERIC_RESY;
    if (fwrite(DG_ScreenBuffer, sizeof(pixel_t), count, frames) != count)
        exit(90);
    if (++frame_count == 350) {
        if (fclose(frames) != 0)
            exit(91);
        fprintf(stderr, "EVOCC_DOOM_FRAMES=350\n");
        exit(0);
    }
}

int main(int argc, char **argv)
{
    int output = dup(STDOUT_FILENO);
    if (output < 0 || dup2(STDERR_FILENO, STDOUT_FILENO) < 0)
        return 92;
    frames = fdopen(output, "wb");
    if (!frames)
        return 93;
    doomgeneric_Create(argc, argv);
    for (;;)
        doomgeneric_Tick();
}
