struct element { int value; };
struct table { int count; struct element items[]; };
struct table trailing = { 1, { { 7 }, {} } };
struct table only_empty = { .items = { {} } };
struct table designated_empty = { .items = { [3] = {} } };
struct table range_empty = { .items = { [2 ... 4] = {}, {} } };

struct pair { int cells[2]; };
struct pair_table { int count; struct pair items[]; };
struct pair_table nested_empty = { .items = { { { 1, 2 } }, {} } };

/* The early arm64 descriptor's trailing empty record is a real sentinel. */
struct field { char name[10]; unsigned char shift, width; void *filter; };
struct descriptor { char name[20]; void *override; struct field fields[]; };
struct descriptor exact = {
    .name = "id_aa64mmfr0",
    .fields = { { .name = "ecv", .shift = 60, .width = 4 }, {} },
};
_Static_assert(sizeof(struct descriptor) == 32, "the type is not completed");

int main(void) {
    return trailing.items[0].value != 7 || trailing.items[1].value != 0
        || only_empty.items[0].value != 0
        || designated_empty.items[3].value != 0 || range_empty.items[5].value != 0
        || nested_empty.items[0].cells[1] != 2 || nested_empty.items[1].cells[1] != 0
        || exact.fields[0].shift != 60 || exact.fields[1].name[0] != 0;
}
