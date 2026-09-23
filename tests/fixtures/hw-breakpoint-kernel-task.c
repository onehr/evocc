// SPDX-License-Identifier: GPL-2.0-only
/* K08 reduction: ARM64 admits a CPU-wide EL1 watchpoint, but intentionally
 * rejects the same kernel address when bound to a task. Reference-built
 * diagnostic module; this is not a replacement for the original KUnit suite. */
#include <linux/hw_breakpoint.h>
#include <linux/module.h>
#include <linux/perf_event.h>
#include <linux/sched.h>

static char watched;

static int __init check_kernel_task_contract(void)
{
    struct perf_event_attr attr;
    struct perf_event *bp;
    int cpu = cpumask_first(cpu_online_mask);
    int targets[] = {-1, cpu};

    hw_breakpoint_init(&attr);
    attr.bp_addr = (unsigned long)&watched;
    attr.bp_len = HW_BREAKPOINT_LEN_1;
    attr.bp_type = HW_BREAKPOINT_RW;

    bp = perf_event_create_kernel_counter(&attr, cpu, NULL, NULL, NULL);
    if (IS_ERR(bp))
        return PTR_ERR(bp);
    unregister_hw_breakpoint(bp);

    for (unsigned int i = 0; i < ARRAY_SIZE(targets); ++i) {
        bp = perf_event_create_kernel_counter(&attr, targets[i], current, NULL, NULL);
        if (!IS_ERR(bp)) {
            unregister_hw_breakpoint(bp);
            return -EUCLEAN;
        }
        if (PTR_ERR(bp) != -EINVAL)
            return PTR_ERR(bp);
    }
    pr_info("HW_BREAKPOINT_CONTRACT cpu-wide=0 task-all=-22 task-cpu=-22\n");
    return 0;
}

static void __exit stop_kernel_task_contract(void) {}
module_init(check_kernel_task_contract);
module_exit(stop_kernel_task_contract);
MODULE_LICENSE("GPL");
MODULE_DESCRIPTION("ARM64 kernel-address breakpoint task-binding contract probe");
