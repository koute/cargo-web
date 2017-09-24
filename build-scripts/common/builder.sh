#!/usr/bin/false

DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"

/bin/bash $DIR/driver.sh

if [ $? != "0" ]; then
    echo "Build failed!"
    exit 1
fi
