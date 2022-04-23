#include <iostream>
#include "nice.h"
#include <cstdio>

hello::Nums nums()
{
    return hello::okok();
}

int main(int, char **)
{

    hello::Nums ns = nums();

    for (int i = 0; i < ns.size; i++)
    {
        *((int*)ns.num + i) = *(ns.num + i) * 100;
    }

    hello::hi_nums(ns);
    return 0;
}
