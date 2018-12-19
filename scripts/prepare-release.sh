strip target/release/dns
cp target/release/dns fly-dns
strip target/release/distributed-fly
cp target/release/distributed-fly fly-dist
tar czf $RELEASE_FILENAME fly-dns fly-dist
mkdir -p release
mv $RELEASE_FILENAME release/