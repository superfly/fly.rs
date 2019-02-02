strip target/release/fly
cp target/release/fly fly
strip target/release/distributed-fly
cp target/release/distributed-fly fly-dist
tar czf $RELEASE_FILENAME fly fly-dist
mkdir -p release
mv $RELEASE_FILENAME release/