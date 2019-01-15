version=$(git rev-parse HEAD | cut -c-8)

if [ -z "$CI" ]; then
  version="$version-dev"
fi

printf "$version"