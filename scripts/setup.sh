#!/bin/bash

set -e

if [[ "$TRAVIS_OS_NAME" == "osx" ]]; then brew update && brew install redis; fi
cd third_party/flatbuffers
cmake -G "Unix Makefiles"
make flatc
cd $TRAVIS_BUILD_DIR

./scripts/fbs.sh

source ~/.nvm/nvm.sh
nvm install 10
npm i -g yarn
yarn install
cd v8env
yarn install
node_modules/.bin/rollup -c
cd $TRAVIS_BUILD_DIR
wget -qO- https://github.com/superfly/libv8/releases/download/7.2.502.13/v8-$TRAVIS_OS_NAME-x64.tar.gz | tar xvz -C libfly