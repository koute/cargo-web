#!/usr/bin/false

NAME = "binaryen"
VERSION = "1.37.27"
RELEASE = "1"

FILES = [
    [
        "https://github.com/WebAssembly/binaryen/archive/#{VERSION}.tar.gz",
        "5f9de1a142eab5e575b7ad663ea37cb81b0eb816e454339594beab23e1c8197a",
        "binaryen-#{VERSION}.tgz"
    ]
]

INSTALL_CMAKE = true
