#include <libc/string.h>
#include <util/util.h>

// TODO Rewrite
ATTR_HOT
int memcmp(const void *s1, const void *s2, size_t n)
{
	size_t i = 0;

	while(((char *) s1)[i] && ((char *) s2)[i] && i < n)
		++i;
	if(i >= n)
		return 0;
	return (((unsigned char *) s1)[i] - ((unsigned char *) s2)[i]);
}