#!/usr/bin/false

NAME = "binaryen"
VERSION = "1.38.19"
RELEASE = "1"

FILES = [
    [
        "https://github.com/WebAssembly/binaryen/archive/#{VERSION}.tar.gz",
        "f5d374c9e7101ebf8f4b30cb8c060ab88b47d7a35b662263df67038f3c38fbea",
        "binaryen-#{VERSION}.tgz"
    ]
]

INSTALL_CMAKE = true
