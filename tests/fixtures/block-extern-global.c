int read_value(void) { extern int value; return value; }
int value = 0;
void write_value(void) { value = 1; }

_Bool read_enabled(void) { extern _Bool enabled; return enabled; }
_Bool enabled = 0;
void enable(void) { enabled = 1; }

int read_array(void) { extern int values[]; return values[3]; }
int values[4] = { 2, 3, 5, 7 };
static int *tail = &values[3];
static int *address = &value;

int local(void) { static int count = 0; return ++count; }

int main(void) {
    write_value();
    enable();
    *tail = 11;
    if (read_value() != 1 || !read_enabled() || *address != 1) return 1;
    if (read_array() != 11 || values[0] != 2) return 2;
    if (local() != 1 || local() != 2) return 3;
    return 0;
}
