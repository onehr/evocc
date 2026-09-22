/* Reduced from SQLite analyzeAggregate: the default arm retains a pointer
 * across the case-dispatch chain and a loop containing a function call. */
struct Source { int count; int cursor; };
struct Expr { unsigned char op; unsigned flags; };
struct Index { struct Expr *expr; int cursor; struct Index *next; };
struct Parse { struct Index *index; };
struct Context { struct Parse *parse; struct Source *source; unsigned flags; };

__attribute__((noinline)) static int compare(struct Expr *a, struct Expr *b, int cursor) {
    return a != b || cursor != 7;
}

static int analyze(struct Context *context, struct Expr *expr) {
    int i;
    struct Parse *parse = context->parse;
    struct Source *source = context->source;
    switch (expr->op) {
        default: {
            struct Index *index;
            if (!(context->flags & 1)) break;
            if (!parse->index) break;
            for (index = parse->index; index; index = index->next) {
                int cursor = index->cursor;
                if (cursor < 0) continue;
                if (compare(expr, index->expr, cursor) == 0) break;
            }
            if (!index) break;
            if (expr->flags) break;
            for (i = 0; i < source->count; ++i) {
                if (source->cursor == index->cursor) break;
            }
            return i < source->count ? 23 : 24;
        }
        case 179:
        case 170:
        case 168:
            if (source) return source->count;
            return 25;
        case 169:
            return 26;
    }
    return 27;
}

int main(void) {
    struct Expr expr = {171, 0};
    struct Index index = {&expr, 7, 0};
    struct Parse parse = {&index};
    struct Source source = {1, 7};
    struct Context context = {&parse, &source, 1};
    if (analyze(&context, &expr) != 23) return 1;
    expr.op = 170;
    return analyze(&context, &expr) != 1;
}
