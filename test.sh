#!/bin/sh

# [tflucke] 2022-01-15: The goal of this script is to provide a safety-net for
# tests accidentally deleting/changing files they shouldn't.
# It will chroot into each test directory with necessary system directories
# mounted as RO before executing do-test.sh.
# Afterwards, the test should revert back to it's original state.

requires="fakechroot faketime bindfs"
not_installed=""

for bin in $requires; do
    if 1 which $bin 1> /dev/null 2> /dev/null; then
        not_installed="$not_installed $bin"
    fi
done
if [ -n "$not_installed" ]; then
    echo "Programs are required to run tests.  Please install:$not_installed" 1> /dev/null 2> /dev/null
    exit 1
fi

directory_list="bin lib lib64 usr/bin"
tsnapshot_srcs=${1:-target/debug}

for test in tests/*; do
    echo $test
    # Initialize a temporary directory
    test_dir=$(mktemp -d "/tmp/tsnapshot-$(basename $test)-XXXXX")
    cp -r $test/. $test_dir/
    # Bind necessary directories to test environment as readonly.
    for dir in $directory_list; do
        mkdir -p $test_dir/$dir
        bindfs -r /$dir $test_dir/$dir
    done
    # Link the tsnapshot executables
    mkdir -p $test_dir/usr/local/bin/
    for app in tsnapshot tsnapshot-restore; do
        cp $tsnapshot_srcs/$app $test_dir/usr/local/bin/
    done
    # Run the test in a fake root environment with a fake timestamp
    if faketime '2000-01-01 00:00:00' fakechroot chroot $test_dir /do-test.sh; then
        echo "\e[1m\e[32m[Success]\e[0m"
        success="y"
    else
        echo "\e[1m\e[31m[Failure]\e[0m"
        echo "Saving generated files in '$test_dir'"
        success=""
    fi
    # Unbind all directories and cleanup.
    for dir in $directory_list; do
        fusermount -u -z $test_dir/$dir
        rmdir $test_dir/$dir
    done
    # Remove the tsnapshot executable links and cleanup
    rm -r $test_dir/usr
    [ -n "$success" ] && rm -r $test_dir
done
