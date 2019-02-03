version="unknown"

if [ -z "$CI" ]; then
  version=${BUILD_VERSION:-${TRAVIS_COMMIT:-${BUILDKITE_COMMIT:-unknown}}}
  version=$(echo -n $version | cut -c-8)
elif [ -x "$(command -v git)" ]; then
  version=$(git rev-parse HEAD | cut -c-8)-dev
fi

printf "$version"