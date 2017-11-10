#!/usr/bin/false

NAME = "binaryen"
VERSION = "1.37.21"
RELEASE = "1"

FILES = [
    [
        "https://github.com/WebAssembly/binaryen/archive/#{VERSION}.tar.gz",
        "37adc9711ec6b40e1c4881ee5b9720021a6c4a7f7f01c6ce2f4d745fcbb3e1d7",
        "binaryen-#{VERSION}.tgz"
    ]
]

INSTALL_CMAKE = true
