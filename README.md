# tsnapshot

A back-up program inspired by rsnapshot

## How to build

```shell
cargo b
```

## How to test

```shell
./test.sh
```

## How to benchmark

_TODO_

## Core Features

* Recursive config files
* Copy, Compress, Hardlink, backups
* High configurable and customizable

## TODO
* Timestamp sanity check al-la `make`
* Remote backup
* Change detection
  * Full file comparison
  * Checksum
* Windows compatibility
  * Zip
* More detailed tests
* CLI config override via argparse
* Benchmarking
