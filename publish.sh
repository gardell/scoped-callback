#!/usr/bin/env bash
# ./publish.sh 0.13.37
set -o errexit -o nounset -o pipefail -o xtrace

VERSION="$1"

cargo_publish () {
    (
        cd $1

        cargo package
        echo "Files added:"
        cargo package --list

        read -r -p "Looks good to publish to crates.io? " response
        case "$response" in
            [yY][eE][sS]|[yY])
                cargo publish
                ;;
            *)
                echo "Aborted"
                exit 6
                ;;
        esac
    )
}

(
    cd "$( dirname "${BASH_SOURCE[0]}" )"

    git fetch
    test -z "$(git status --porcelain)" || (echo "Dirty repo"; exit 2)
    test -z "$(git diff origin/master)" || (echo "Not up to date with origin/master"; exit 3)
    test -z "$(./generate_readme.sh | diff - README.md)" || (echo "README.md not up to date"; exit 4)

    ./test.sh

    cargo fmt -- --check

    git fetch --tags
    git tag -l | sed '/^'"${VERSION}"'$/{q2}' > /dev/null \
        || (echo "${VERSION} already exists"; exit 5)

    find . \
        -iname Cargo.toml \
        -not -path "./target/*" \
        -exec sed -i 's/^version = .*$/version = "'"${VERSION}"'"/g' '{}' \; \
        -exec git add '{}' \;

    git diff origin/master

    read -r -p "Deploying ${VERSION}, are you sure? [y/N]? " response
    case "$response" in
        [yY][eE][sS]|[yY])
            git commit -m"Version ${VERSION}"
            git tag "${VERSION}"
            git push origin "${VERSION}"
            git push origin master
            cargo_publish .
            ;;
        *)
            git checkout .
            echo "Aborted"
            ;;
    esac
)
