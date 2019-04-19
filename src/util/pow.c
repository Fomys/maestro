#include "util.h"

unsigned pow2(const unsigned n)
{
	if(n == 0) return 1;
	unsigned i = 0, r = 2;

	while(i < n)
	{
		r *= 2;
		++i;
	}

	return r;
}
