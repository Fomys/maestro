#include <memory/memory.h>
#include <elf/elf.h>
#include <libc/errno.h>

// TODO Use `kernel_vmem` to hide holes in memory?

/*
 * This file handles x86 memory permissions handling.
 * x86 uses a tree-like structure to handle permissions. This structure is made
 * of several elements:
 * - Page directory: A 1024 entries array containing page tables
 * - Page table: A 1024 entries array describing permissions on each page
 *
 * Both objects are 4096 bytes large.
 */

/*
 * The kernel's memory context.
 */
vmem_t kernel_vmem;

/*
 * Creates a paging object.
 */
ATTR_HOT
static inline vmem_t new_vmem_obj(void)
{
	return buddy_alloc_zero(0);
}

/*
 * Initializes a new page directory. By default, the page directory is a copy
 * of the kernel's page directory.
 */
ATTR_HOT
vmem_t vmem_init(void)
{
	vmem_t vmem;

	if(!(vmem = new_vmem_obj()))
		return NULL;
	// TODO Only allow read access to kernel (or just a stub for interrupts)
	memcpy(vmem, kernel_vmem, PAGE_SIZE); // TODO rm
	return vmem;
}

/*
 * Protects write-protected section specified by the ELF section header given
 * in argument.
 */
ATTR_COLD
static void protect_section(elf_section_header_t *hdr, const char *name)
{
	void *ptr;
	size_t pages;

	(void) name;
	if(hdr->sh_flags & SHF_WRITE || hdr->sh_addralign != PAGE_SIZE)
		return;
	ptr = (void *) hdr->sh_addr;
	pages = CEIL_DIVISION(hdr->sh_size, PAGE_SIZE);
	vmem_identity_range(kernel_vmem, ptr, ptr + (pages * PAGE_SIZE),
		PAGING_PAGE_USER);
}

/*
 * Protects the kernel code.
 */
ATTR_COLD
static void protect_kernel(void)
{
	iterate_sections(boot_info.elf_sections, boot_info.elf_num,
		boot_info.elf_shndx, boot_info.elf_entsize, protect_section);
}

/*
 * Creates the kernel's page directory.
 */
ATTR_COLD
void vmem_kernel(void)
{
	if(!(kernel_vmem = new_vmem_obj()))
		goto fail;
	vmem_unmap(kernel_vmem, NULL);
	vmem_identity_range(kernel_vmem, (void *) PAGE_SIZE, mem_info.memory_end,
		PAGING_PAGE_WRITE);
	protect_kernel();
	paging_enable(kernel_vmem);
	return;

fail:
	PANIC("Cannot initialize kernel virtual memory!", 0);
}

/*
 * Identity mapping of the given page. (maps the given page to the same virtual
 * address as its physical address)
 */
ATTR_HOT
void vmem_identity(vmem_t vmem, void *page, const int flags)
{
	vmem_map(vmem, page, page, flags);
}

/*
 * Identity maps a range of pages.
 */
ATTR_HOT
void vmem_identity_range(vmem_t vmem, void *from, void *to, int flags)
{
	void *ptr;

	if(!vmem)
		return;
	for(ptr = from; ptr < to; ptr += PAGE_SIZE)
	{
		vmem_identity(vmem, ptr, flags);
		if(errno)
		{
			// TODO Unmap range
		}
	}
}

/*
 * Resolves the paging entry for the given pointer. If no entry is found, `NULL`
 * is returned. The entry must be marked as present to be found.
 */
ATTR_HOT
uint32_t *vmem_resolve(vmem_t vmem, void *ptr)
{
	uintptr_t table, page;
	vmem_t table_obj;

	table = ADDR_TABLE(ptr);
	page = ADDR_PAGE(ptr);
	if(!(vmem[table] & PAGING_TABLE_PRESENT))
		return NULL;
	table_obj = (void *) (vmem[table] & PAGING_ADDR_MASK);
	if(!(table_obj[page] & PAGING_PAGE_PRESENT))
		return NULL;
	return table_obj + page;
}

/*
 * Checks if the given pointer is mapped.
 */
ATTR_HOT
int vmem_is_mapped(vmem_t vmem, void *ptr)
{
	return (vmem_resolve(vmem, ptr) != NULL);
}

// TODO Reload tlb after mapping?
/*
 * Maps the given physical address to the given virtual address with the given
 * flags.
 */
ATTR_HOT
void vmem_map(vmem_t vmem, void *physaddr, void *virtaddr, const int flags)
{
	size_t t;
	vmem_t v;

	if(!vmem)
		return;
	t = ADDR_TABLE(virtaddr);
	if(!(vmem[t] & PAGING_TABLE_PRESENT))
	{
		if(!(v = new_vmem_obj()))
			return;
		vmem[t] = (uintptr_t) v;
	}
	vmem[t] |= PAGING_TABLE_PRESENT | flags;
	v = (void *) (vmem[t] & PAGING_ADDR_MASK);
	v[ADDR_PAGE(virtaddr)] = (uintptr_t) physaddr | PAGING_PAGE_PRESENT | flags;
}

/*
 * Unmaps the given virtual address.
 */
ATTR_HOT
void vmem_unmap(vmem_t vmem, void *virtaddr)
{
	size_t t;
	vmem_t v;

	if(!vmem)
		return;
	t = ADDR_TABLE(virtaddr);
	if(!(vmem[t] & PAGING_TABLE_PRESENT))
		return;
	v = (void *) (vmem[t] & PAGING_ADDR_MASK);
	v[ADDR_PAGE(virtaddr)] = 0;
	// TODO If page table is empty, free it
}

/*
 * Checks if the portion of memory beginning at `ptr` with size `size` is
 * mapped.
 */
ATTR_HOT
int vmem_contains(vmem_t vmem, const void *ptr, const size_t size)
{
	void *i;

	if(!vmem)
		return 0;
	i = DOWN_ALIGN(ptr, PAGE_SIZE);
	while(i < ptr + size)
	{
		if(!vmem_is_mapped(vmem, i))
			return 0;
		i += PAGE_SIZE;
	}
	return 1;
}

/*
 * Translates the given virtual address to the corresponding physical address.
 * If the address is not mapped, `NULL` is returned.
 */
ATTR_HOT
void *vmem_translate(vmem_t vmem, void *ptr)
{
	uint32_t *entry;

	if(!vmem || !(entry = vmem_resolve(vmem, ptr)))
		return NULL;
	return (void *) ((*entry & PAGING_ADDR_MASK) | ADDR_REMAIN(ptr));
}

/*
 * Resolves the entry for the given virtual address and returns its flags.
 */
ATTR_HOT
uint32_t vmem_get_entry(vmem_t vmem, void *ptr)
{
	uint32_t *entry;

	if(!vmem || !(entry = vmem_resolve(vmem, ptr)))
		return 0;
	return *entry & PAGING_FLAGS_MASK;
}

/*
 * Clones the given page table.
 */
ATTR_HOT
static vmem_t clone_page_table(vmem_t from)
{
	vmem_t v;

	if(!from || !(v = new_vmem_obj()))
		return NULL;
	memcpy(v, from, PAGE_SIZE);
	return v;
}

/*
 * Clones the given page directory.
 */
ATTR_HOT
vmem_t vmem_clone(vmem_t vmem)
{
	vmem_t v;
	size_t i;
	void *old_table, *new_table;

	if(!vmem || !(v = vmem_init()))
		return NULL;
	for(i = 0; i < 1024; ++i)
	{
		if(!(vmem[i] & PAGING_TABLE_PRESENT))
			continue;
		if((vmem[i] & PAGING_TABLE_USER))
		{
			old_table = (void *) (vmem[i] & PAGING_ADDR_MASK);
			if(!(new_table = clone_page_table(old_table)))
				goto fail;
			v[i] = ((uint32_t) new_table) | (vmem[i] & PAGING_FLAGS_MASK);
		}
		else
			v[i] = vmem[i];
	}
	return v;

fail:
	vmem_destroy(v);
	return NULL;
}

/*
 * Destroyes the given page directory.
 */
ATTR_HOT
void vmem_destroy(vmem_t vmem)
{
	size_t i;

	if(!vmem)
		return;
	for(i = 0; i < 1024; ++i)
	{
		if(!(vmem[i] & PAGING_TABLE_PRESENT))
			continue;
		buddy_free((void *) (vmem[i] & PAGING_ADDR_MASK));
	}
	buddy_free(vmem);
}
