/* A reduction of start() in musl's src/time/timer_create.c, without
   headers. setjmp is in a loop, and the values that live across the call
   also live around the loop. The asm statement after the loop, with operands
   in fixed registers, gives the allocation of the original: each iteration
   copies the kept values, and a spill store of each copy writes its private
   slot before the copy itself. */
typedef long jmp_buf[32];
int setjmp(jmp_buf);
_Noreturn void longjmp(jmp_buf, int);

struct self {
	int cancel;
	int timer_id;
};
union sigval {
	int sival_int;
	void *sival_ptr;
};
struct sigevent {
	union sigval sigev_value;
	void (*sigev_notify_function)(union sigval);
};
struct start_args {
	long sem1, sem2;
	struct sigevent *sev;
};
struct info {
	int si_signo;
	int si_errno;
	int si_code;
};

struct self *current(void);
int wait_one(long *);
void post_one(long *);
int next_signal(const unsigned long *, struct info *);
void push(void (*)(void *), void *);
void pop(int);

static inline long finish(long n, long a)
{
	register long x8 __asm__("x8") = n;
	register long x0 __asm__("x0") = a;
	__asm__ __volatile__("" : "=r"(x0) : "r"(x8), "0"(x0) : "memory", "cc");
	return x0;
}

static void cleanup(void *p)
{
	longjmp(p, 1);
}

void *start(void *arg)
{
	struct self *self = current();
	struct start_args *args = arg;
	jmp_buf jb;
	void (*notify)(union sigval) = args->sev->sigev_notify_function;
	union sigval val = args->sev->sigev_value;

	while (wait_one(&args->sem1));
	post_one(&args->sem2);
	if (self->cancel)
		return 0;
	for (;;) {
		struct info si;
		unsigned long set = 1UL << 31;
		while (next_signal(&set, &si) < 0);
		if (si.si_code == -2 && !setjmp(jb)) {
			push(cleanup, jb);
			notify(val);
			pop(1);
		}
		if (self->timer_id < 0)
			break;
	}
	finish(111, self->timer_id & 0x7fffffff);
	return 0;
}
