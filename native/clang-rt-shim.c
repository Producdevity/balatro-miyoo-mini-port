/* LuaJIT built with Zig's clang calls the generic 64-bit conversion helpers.
   GCC's libgcc only exports the ARM EABI names, so forward them here. */
double __floatdidf(long long a) { return (double)a; }
double __floatundidf(unsigned long long a) { return (double)a; }
float __floatdisf(long long a) { return (float)a; }
float __floatundisf(unsigned long long a) { return (float)a; }
long long __fixdfdi(double a) { return (long long)a; }
unsigned long long __fixunsdfdi(double a) { return (unsigned long long)a; }
long long __fixsfdi(float a) { return (long long)a; }
unsigned long long __fixunssfdi(float a) { return (unsigned long long)a; }
