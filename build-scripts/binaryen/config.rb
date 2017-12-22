#!/usr/bin/false

NAME = "binaryen"
VERSION = "1.37.26"
RELEASE = "1"

FILES = [
    [
        "https://github.com/WebAssembly/binaryen/archive/#{VERSION}.tar.gz",
        "4fbc9945f96ed7e1489236ca9b4b5771a8181fa9037437f396ea40349dbe66ce",
        "binaryen-#{VERSION}.tgz"
    ]
]

INSTALL_CMAKE = true
