/* Written for this corpus; see origin.md for the algorithm contract. */
/* Periodic scheduler for eight distinct IDs over modulo-2^32 unsigned ticks.
 * Delay/advance are 0..1000000 and periods are 1..1000000; active deadlines
 * stay within half the tick ring. Each advance counts every elapsed period
 * and leaves active deadlines strictly in the future.
 * Add rejects duplicate IDs/full storage; add/restart reject invalid bounds.
 * Cancellation retains firing history; a null optional scheduler is a no-op.
 * Firing counts/digests are unsigned 64-bit. Only evaluate seeds 0..11 are
 * supported.
 */
typedef struct {
    unsigned id;
    unsigned deadline;
    unsigned period;
    unsigned active;
    unsigned long long fired;
} Timer;
typedef struct {
    Timer timers[8];
    unsigned now;
    unsigned count;
} Scheduler;
static int time_due(unsigned now, unsigned deadline)
{
    unsigned distance = now - deadline;
    return distance < 0x80000000U;
}
static void scheduler_init(Scheduler *scheduler, unsigned now)
{
    unsigned i;
    scheduler->now = now;
    scheduler->count = 0;
    for (i = 0; i < 8; ++i) {
        scheduler->timers[i].id = 0;
        scheduler->timers[i].deadline = now;
        scheduler->timers[i].period = 0;
        scheduler->timers[i].active = 0;
        scheduler->timers[i].fired = 0;
    }
}
static int scheduler_find(const Scheduler *scheduler, unsigned id)
{
    unsigned i;
    for (i = 0; i < scheduler->count; ++i)
        if (scheduler->timers[i].id == id)
            return (int)i;
    return -1;
}
static int scheduler_add(Scheduler *scheduler, unsigned id,
                         unsigned delay, unsigned period)
{
    Timer *timer;
    if (scheduler->count == 8 || scheduler_find(scheduler, id) >= 0)
        return 0;
    if (delay > 1000000 || period == 0 || period > 1000000)
        return 0;
    timer = &scheduler->timers[scheduler->count++];
    timer->id = id;
    timer->deadline = scheduler->now + delay;
    timer->period = period;
    timer->active = 1;
    timer->fired = 0;
    return 1;
}
static int scheduler_cancel(Scheduler *scheduler, unsigned id)
{
    int index = scheduler_find(scheduler, id);
    if (index < 0)
        return 0;
    scheduler->timers[index].active = 0;
    return 1;
}
static int scheduler_restart(Scheduler *scheduler, unsigned id,
                             unsigned delay, unsigned period)
{
    int index = scheduler_find(scheduler, id);
    Timer *timer;
    if (index < 0 || delay > 1000000 || period == 0 || period > 1000000)
        return 0;
    timer = &scheduler->timers[index];
    timer->deadline = scheduler->now + delay;
    timer->period = period;
    timer->active = 1;
    return 1;
}
static unsigned long long scheduler_advance(Scheduler *scheduler, unsigned elapsed)
{
    unsigned i;
    unsigned long long delivered = 0;
    if (elapsed > 1000000)
        return 0;
    scheduler->now += elapsed;
    for (i = 0; i < scheduler->count; ++i) {
        Timer *timer = &scheduler->timers[i];
        if (timer->active && time_due(scheduler->now, timer->deadline)) {
            unsigned lateness = scheduler->now - timer->deadline;
            unsigned steps = lateness / timer->period + 1;
            timer->fired += steps;
            delivered += steps;
            timer->deadline += timer->period;
        }
    }
    return delivered;
}
static unsigned scheduler_next_delay(const Scheduler *scheduler)
{
    unsigned i;
    unsigned delay = 0xffffffffU;
    for (i = 0; i < scheduler->count; ++i) {
        const Timer *timer = &scheduler->timers[i];
        if (timer->active) {
            unsigned candidate = time_due(scheduler->now, timer->deadline) ?
                                 0 : timer->deadline - scheduler->now;
            if (candidate < delay)
                delay = candidate;
        }
    }
    return delay;
}
static unsigned long long scheduler_digest(const Scheduler *scheduler)
{
    unsigned i;
    unsigned long long digest = scheduler->now;
    for (i = 0; i < scheduler->count; ++i) {
        const Timer *timer = &scheduler->timers[i];
        digest = digest * 1099511628211ULL ^ timer->id;
        digest = digest * 1099511628211ULL ^ timer->deadline;
        digest = digest * 1099511628211ULL ^ timer->period;
        digest = digest * 1099511628211ULL ^ timer->active;
        digest = digest * 1099511628211ULL ^ timer->fired;
    }
    return digest;
}
unsigned long long evaluate(unsigned seed)
{
    Scheduler scheduler;
    unsigned start = seed == 11 ? 0x7fffffe0U : 100 + seed * 1009;
    unsigned i;
    unsigned long long events = 0;
    unsigned long long digest;
    scheduler_init(&scheduler, start);
    for (i = 0; i < 6; ++i)
        scheduler_add(&scheduler, i + 1, i * 3 + seed % 5, 3 + i + seed % 7);
    events += scheduler_advance(&scheduler, 2);
    events += scheduler_advance(&scheduler, 5 + seed);
    scheduler_cancel(&scheduler, 3);
    events += scheduler_advance(&scheduler, 37 + seed * 2);
    scheduler_restart(&scheduler, 3, seed % 7, 11 + seed);
    digest = scheduler_digest(&scheduler) ^ scheduler_next_delay(&scheduler);
    for (i = 0; i < 5; ++i) {
        events += scheduler_advance(&scheduler, 17 + i * 7 + seed);
        digest = digest * 33 ^ scheduler_digest(&scheduler);
        digest += scheduler_next_delay(&scheduler);
    }
    events += scheduler_cancel(seed == 11 ? 0 : &scheduler, 7);
    return digest ^ events;
}
int main(void)
{
    unsigned long long checksum = evaluate(0);
    checksum ^= evaluate(1);
    checksum += evaluate(2);
    checksum ^= evaluate(11);
    return checksum == 0xffffffffffffffffULL;
}
