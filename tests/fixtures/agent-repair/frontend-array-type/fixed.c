int array_pair(int first, int second)
{
    int arr[2];

    arr[0] = first;
    arr[1] = second;
    return arr[0] + arr[1] - 3;
}

int main(void)
{
    return array_pair(1, 2);
}
