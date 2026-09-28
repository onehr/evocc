/* Reference-built observation adapter, not part of any EvoCC game claim.
 * Read the real SDL/OpenGL framebuffer before presentation. Inject only a
 * documented Return key and a normal SDL_QUIT event, never game state/pixels.
 */
#define _GNU_SOURCE
#include <SDL.h>
#include <GL/gl.h>
#include <dlfcn.h>
#include <stdio.h>
#include <stdlib.h>

static unsigned frame;

static void events(void)
{
    SDL_Event e = {0};
    const char *key = getenv("EVO_CAPTURE_RETURN");
    if (key && (frame == strtoul(key, 0, 10) || frame == strtoul(key, 0, 10) + 1)) {
        e.type = frame == strtoul(key, 0, 10) ? SDL_KEYDOWN : SDL_KEYUP;
        e.key.state = e.type == SDL_KEYDOWN ? SDL_PRESSED : SDL_RELEASED;
        e.key.keysym.sym = SDLK_RETURN;
        e.key.keysym.scancode = SDL_SCANCODE_RETURN;
        SDL_PushEvent(&e);
    }
    const char *end = getenv("EVO_CAPTURE_END");
    if (end && frame == strtoul(end, 0, 10)) {
        e.type = SDL_QUIT;
        SDL_PushEvent(&e);
    }
}

static void capture(SDL_Renderer *renderer, SDL_Window *window)
{
    ++frame;
    /* Keep only three frames, never a disk-filling video stream. */
    if (frame == 180 || frame == 360 || frame == 600) {
        int w = 0, h = 0;
        if (renderer) SDL_GetRendererOutputSize(renderer, &w, &h);
        else SDL_GL_GetDrawableSize(window, &w, &h);
        if (w <= 0 || h <= 0 || w > 4096 || h > 4096) abort();
        unsigned char *pixels = malloc((size_t)w * h * 3);
        if (!pixels) abort();
        if (renderer) {
            if (SDL_RenderReadPixels(renderer, NULL, SDL_PIXELFORMAT_RGB24, pixels, w * 3)) abort();
        } else {
            GLint alignment;
            glGetIntegerv(GL_PACK_ALIGNMENT, &alignment);
            glPixelStorei(GL_PACK_ALIGNMENT, 1);
            glReadPixels(0, 0, w, h, GL_RGB, GL_UNSIGNED_BYTE, pixels);
            glPixelStorei(GL_PACK_ALIGNMENT, alignment);
        }
        const char *dir = getenv("EVO_CAPTURE_DIR");
        if (!dir) abort();
        char path[4096];
        if (snprintf(path, sizeof(path), "%s/frame-%u.ppm", dir, frame) >= (int)sizeof(path)) abort();
        FILE *out = fopen(path, "wb");
        if (!out) abort();
        fprintf(out, "P6\n%d %d\n255\n", w, h);
        for (int y = 0; y < h; ++y) {
            int source_y = renderer ? y : h - y - 1;
            if (fwrite(pixels + (size_t)source_y * w * 3, 3, w, out) != (size_t)w) abort();
        }
        if (fclose(out)) abort();
        free(pixels);
        fprintf(stderr, "EVO_CAPTURE frame=%u width=%d height=%d\n", frame, w, h);
    }
    events();
}

void SDL_RenderPresent(SDL_Renderer *renderer)
{
    static void (*next)(SDL_Renderer *);
    if (!next) next = dlsym(RTLD_NEXT, "SDL_RenderPresent");
    if (!next) abort();
    capture(renderer, NULL);
    next(renderer);
}

void SDL_GL_SwapWindow(SDL_Window *window)
{
    static void (*next)(SDL_Window *);
    if (!next) next = dlsym(RTLD_NEXT, "SDL_GL_SwapWindow");
    if (!next) abort();
    capture(NULL, window);
    next(window);
}
