#!/usr/bin/false

NAME = "emscripten"
VERSION = "1.38.19"
RELEASE = "1"

FILES = [
    [
        "https://github.com/kripken/emscripten/archive/#{VERSION}.tar.gz",
        "4bdb7932f084171e40b405b4ab5e60aa7adb36ae399ba88c967e66719fc2d1e2",
        "emscripten-#{VERSION}.tgz"
    ],
    [
        "https://github.com/kripken/emscripten-fastcomp/archive/#{VERSION}.tar.gz",
        "19943b4299e4309fc7810e785ee0e38a15059c7a54d9451b2e0ed29f9573a29d",
        "emscripten_fastcomp-#{VERSION}.tgz"
    ],
    [
        "https://github.com/kripken/emscripten-fastcomp-clang/archive/#{VERSION}.tar.gz",
        "fbfb90f5d521fec143952a1b261a55ced293551f6753768f80499fb87bd876ca",
        "emscripten_fastcomp_clang-#{VERSION}.tgz"
    ]
]

INSTALL_CMAKE = true
