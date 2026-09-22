extern void event(int);
static int state;
static void first(void) __attribute__((constructor(201)));
static void second(void) __attribute__((constructor(202)));
static void final_second(void) __attribute__((destructor(202)));
static void final_first(void) __attribute__((destructor(201)));
static void first(void){state=1;event(1);}
static void second(void){if(state!=1)__builtin_trap();state=2;event(2);}
void lifecycle_touch(void){if(state!=2)__builtin_trap();state=3;event(3);}
static void final_second(void){if(state!=3)__builtin_trap();state=4;event(4);}
static void final_first(void){if(state!=4)__builtin_trap();event(5);}
