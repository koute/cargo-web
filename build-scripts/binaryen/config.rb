#!/usr/bin/false

NAME = "binaryen"
VERSION = "1.38.11"
RELEASE = "1"

FILES = [
    [
        "https://github.com/WebAssembly/binaryen/archive/#{VERSION}.tar.gz",
        "414a3dd59876d86095aa91970dc1b2a8a3aafacc7e38388768b8bae4b320bd00",
        "binaryen-#{VERSION}.tgz"
    ]
]

INSTALL_CMAKE = true
