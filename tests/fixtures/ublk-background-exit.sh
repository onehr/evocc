#!/bin/bash
# SPDX-License-Identifier: GPL-2.0
# Reduction of Linux selftests/ublk/test_stress_04.sh: ERR_CODE belongs
# to each background subshell; wait without operands does not forward failure.
# No device, module, root permission, sleep, or real I/O is needed.
TID=stress_04
ERR_CODE=0
worker() {
    if [ "$1" = helper-exit ]; then
        # test_common.sh can exit here before its caller prints [FAIL].
        echo "/dev/ublkc7 isn't removed res "
        exit 255
    fi
    if [ "$1" = failure ]; then
        false
        ERR_CODE=$?
        echo "$TID : [FAIL]"
        exit "$ERR_CODE"
    fi
}
worker "$1" &
wait
if [ "$ERR_CODE" -eq 0 ]; then
    echo "$TID : [PASS]"
fi
exit "$ERR_CODE"
