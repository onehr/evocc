/* Exercise the real pinned libasound cleanup with a device-free timer backend.
 * This is a userspace dependency regression, not kernel/compiler acceptance. */
#include "timer/timer_local.h"

static unsigned enable_count, disable_count, close_count;

static int timer_async(snd_timer_t *timer, int sig, pid_t pid)
{
    (void)timer;
    assert(pid > 0);
    if (sig < 0)
        disable_count++;
    else
        enable_count++;
    return 0;
}

static int timer_close(snd_timer_t *timer)
{
    assert(list_empty(&timer->async_handlers));
    close_count++;
    return 0;
}

int main(void)
{
    static const snd_timer_ops_t ops = {.async = timer_async, .close = timer_close};
    snd_timer_t *timer = calloc(1, sizeof(*timer));
    snd_async_handler_t *first, *second;
    assert(timer);
    timer->ops = &ops;
    INIT_LIST_HEAD(&timer->async_handlers);
    assert(snd_async_add_timer_handler(&first, timer, NULL, NULL) == 0);
    assert(snd_async_add_timer_handler(&second, timer, NULL, NULL) == 0);
    assert(enable_count == 1);
    assert(snd_async_del_handler(first) == 0);
    assert(disable_count == 0 && !list_empty(&timer->async_handlers));
    assert(snd_async_del_handler(second) == 0);
    assert(disable_count == 1 && list_empty(&timer->async_handlers));
    assert(snd_async_add_timer_handler(&first, timer, NULL, NULL) == 0);
    assert(enable_count == 2);
    assert(snd_timer_close(timer) == 0);
    assert(disable_count == 2 && close_count == 1);
    return 0;
}
