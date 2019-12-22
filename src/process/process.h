#ifndef PROCESS_H
# define PROCESS_H

# include <gdt.h>
# include <memory/memory.h>
# include <process/tss.h>
# include <process/signal.h>

# define PID_MAX			32768
# define PIDS_BITMAP_SIZE	(PID_MAX / BIT_SIZEOF(char))

__attribute__((packed))
struct regs
{
	int32_t ebp;
	int32_t esp;
	int32_t eip;
	int32_t eflags;
	int32_t eax;
	int32_t ebx;
	int32_t ecx;
	int32_t edx;
	int32_t esi;
	int32_t edi;
};

typedef struct regs regs_t;

typedef enum
{
	WAITING,
	RUNNING,
	BLOCKED,
	STOPPED,
	TERMINATED
} process_state_t;

typedef struct
{
	process_t *proc_current;
	process_t *proc_queue;
} semaphore_t;

typedef struct child child_t;

typedef struct process
{
	struct process *next;

	pid_t pid;
	uid_t owner_id;
	process_state_t state, prev_state;

	struct process *parent;
	child_t *children;

	semaphore_t *sem_curr;
	struct process *sem_next;

	mem_space_t *mem_space;
	void *user_stack;
	void *kernel_stack;
	regs_t regs_state;
	char syscalling;

	sigaction_t sigactions[SIG_MAX];
	signal_t *signals_queue, *last_signal;
	int status;

	spinlock_t spinlock;
} process_t;

struct child
{
	struct child *next;
	process_t *process;
};

void sem_init(semaphore_t *sem);
void sem_wait(semaphore_t *sem, process_t *process);
void sem_remove(semaphore_t *sem, process_t *process);
void sem_post(semaphore_t *sem);

extern gdt_entry_t *tss_gdt_entry(void);
extern void tss_flush(void);

void process_init(void);
process_t *new_process(process_t *parent, const regs_t *registers);
process_t *get_process(pid_t pid);
process_t *get_running_process(void);
process_t *process_clone(process_t *proc);
void process_set_state(process_t *process, process_state_t state);
void process_add_child(process_t *parent, process_t *child);
void process_exit(process_t *proc, int status);
void process_kill(process_t *proc, int sig);
void del_process(process_t *process, int children);

void process_tick(const regs_t *registers);

__attribute__((noreturn))
extern void context_switch(const regs_t *regs,
	uint16_t data_selector, uint16_t code_selector);
__attribute__((noreturn))
extern void kernel_switch(const regs_t *regs);

#endif
