// SPDX-License-Identifier: GPL-2.0-only
/* K05 reduction: pm_runtime_get queues a resume; its worker may finish before
 * the caller reaches pm_runtime_barrier. flush_work selects that legal ordering
 * deterministically. This does not change the frozen KUnit assertion. */
#include <linux/device.h>
#include <linux/module.h>
#include <linux/pm_runtime.h>
#include <linux/workqueue.h>

static int __init check_completed_resume(void)
{
    struct device *dev = root_device_register("pm-barrier-contract");
    unsigned int completed_zero = 0, immediate_zero = 0, immediate_one = 0;
    int result = 0;

    if (IS_ERR(dev))
        return PTR_ERR(dev);
    pm_runtime_no_callbacks(dev);
    pm_runtime_enable(dev);

    for (unsigned int i = 0; i < 64; ++i) {
        int ret = pm_runtime_get(dev);
        if (ret < 0) {
            result = ret;
            break;
        }
        if (!(i & 1))
            flush_work(&dev->power.work);
        ret = pm_runtime_barrier(dev);
        if (!(i & 1)) {
            if (ret != 0)
                result = -EUCLEAN;
            else
                ++completed_zero;
        } else if (ret == 0) {
            ++immediate_zero;
        } else if (ret == 1) {
            ++immediate_one;
        } else {
            result = -EUCLEAN;
        }
        if (!pm_runtime_active(dev))
            result = -EUCLEAN;
        ret = pm_runtime_put_sync_suspend(dev);
        if (ret < 0 || !pm_runtime_suspended(dev))
            result = -EUCLEAN;
        if (result)
            break;
    }
    pm_runtime_disable(dev);
    root_device_unregister(dev);
    pr_info("PM_BARRIER completed_zero=%u immediate_zero=%u immediate_one=%u result=%d\n",
            completed_zero, immediate_zero, immediate_one, result);
    return result;
}

static void __exit stop_completed_resume(void) {}
module_init(check_completed_resume);
module_exit(stop_completed_resume);
MODULE_LICENSE("GPL");
MODULE_DESCRIPTION("Runtime PM completed asynchronous resume/barrier contract");
