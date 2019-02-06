commit_sha=${BUILD_VERSION:-${TRAVIS_COMMIT:-${BUILDKITE_COMMIT:-$(git rev-parse HEAD)}}}
version=$(echo $commit_sha | cut -c-8)

if [ -z "$CI" ]; then
  version="$version-dev"
fi

printf $version