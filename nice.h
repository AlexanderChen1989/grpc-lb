#include <cstdarg>
#include <cstddef>
#include <cstdint>
#include <cstdlib>
#include <ostream>
#include <new>


namespace hello {

struct Nums {
  const int *num;
  size_t cap;
  size_t size;
};


extern "C" {

void hello();

void hi_nums(Nums nums);

const char *name();

Nums okok();

void say_hi(const char *name);

void say_nums(const int *name, int size);

} // extern "C"

} // namespace hello
