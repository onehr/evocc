"""A dead orphan in the launcher's group; PID 1 may intentionally not reap it."""
import os
import time

if os.fork() == 0:
    if os.fork() == 0:
        os._exit(0)
    os._exit(0)
# Wait for all group descendants to die, but do not reap the intermediate child.
# The capture reaps this leader; the remaining members are only zombies.
time.sleep(0.1)
