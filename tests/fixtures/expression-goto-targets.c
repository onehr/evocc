/* The scoped_guard loop step jumps into the otherwise unselected if arm. */
int loop_step(void)
{
    int count = 0;
    for (; 1; ({ goto done; }))
        if (0) {
done:
            break;
        } else {
            ++count;
        }
    return count;
}

int backward_expression(void)
{
    int count = 0;
    if (0) {
again:
        ++count;
        if (count == 3)
            return count;
    }
    ({ goto again; });
    return 99;
}

int initializer_expression(int jump)
{
    int value = ({ if (jump) goto done; 7; });
    if (0) {
done:
        return 9;
    }
    return value;
}

int main(void)
{
    return loop_step() != 1 || backward_expression() != 3 ||
           initializer_expression(0) != 7 || initializer_expression(1) != 9;
}
