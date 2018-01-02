#!/usr/bin/false

NAME = "emscripten"
VERSION = "1.37.27"
RELEASE = "1"

FILES = [
    [
        "https://github.com/kripken/emscripten/archive/#{VERSION}.tar.gz",
        "a345032415362a0a66e4886ecd751f6394237ff764b1f1c40dde25410792991c",
        "emscripten-#{VERSION}.tgz"
    ],
    [
        "https://github.com/kripken/emscripten-fastcomp/archive/#{VERSION}.tar.gz",
        "409055d32dca9788b7ef15fbe81bd1df82a0ab91337f15be3254c11d5743043a",
        "emscripten_fastcomp-#{VERSION}.tgz"
    ],
    [
        "https://github.com/kripken/emscripten-fastcomp-clang/archive/#{VERSION}.tar.gz",
        "bd532912eab4e52bd83f603c7fb4d2fe770b99ede766e2b9a82f5f3f68f4a168",
        "emscripten_fastcomp_clang-#{VERSION}.tgz"
    ]
]

INSTALL_CMAKE = true
