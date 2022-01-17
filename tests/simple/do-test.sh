#!/bin/sh

mkdir -p /mnt/backup/ /mnt/restore/
tsnapshot /etc/simpleConfig.json
#find / -name catalog.txt
tsnapshot-restore /etc/simpleConfig.json /mnt/restore
diff -r /home /mnt/restore/home
exit $?

