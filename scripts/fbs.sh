#!/bin/bash

set -e

flatc --rust -o src --gen-all msg.fbs
flatc --ts -o v8env/src --no-fb-import --gen-mutable --no-ts-reexport --gen-all msg.fbs