static int choose(int input) {
    return ({
        __label__ failed, failed_begin;
        int result = 7;
        if (input < 0) goto failed_begin;
        if (input) goto failed;
        result = 11;
failed:
        result += 3;
failed_begin:
        result;
    });
}

int main(void) {
    __label__ done, other;
    int result = 0;
    {
        __label__ done, other;
        goto other;
done:
        result = 99;
        goto other;
other:
        result += 1;
    }
    if (choose(-1) != 7 || choose(1) != 10 || choose(0) != 14) goto other;
    goto done;
other:
    return 2;
done:
    return result != 1;
}
