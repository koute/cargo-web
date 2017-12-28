use std::process::exit;
use std::path::{Path, PathBuf};
use std::env;

use package::{
    PrebuiltPackage,
    download_package
};
use utils::check_if_command_exists;

fn emscripten_package() -> Option< PrebuiltPackage > {
    let package =
        if cfg!( target_os = "linux" ) && cfg!( target_arch = "x86_64" ) {
            PrebuiltPackage {
                url: "https://github.com/koute/emscripten-build/releases/download/emscripten-1.37.26-1/emscripten-1.37.26-1-x86_64-unknown-linux-gnu.tgz",
                name: "emscripten",
                version: "1.37.26-1",
                arch: "x86_64-unknown-linux-gnu",
                hash: "0b8392bf6b22f13b99bfedeff2d0d1eae2bbd876e796f9b01468179facd66a00",
                size: 136903726
            }
        } else if cfg!( target_os = "linux" ) && cfg!( target_arch = "x86" ) {
            PrebuiltPackage {
                url: "https://github.com/koute/emscripten-build/releases/download/emscripten-1.37.26-1/emscripten-1.37.26-1-i686-unknown-linux-gnu.tgz",
                name: "emscripten",
                version: "1.37.26-1",
                arch: "i686-unknown-linux-gnu",
                hash: "3cfe8c59812fb9bc2c61c21ce18158811af36dbb31229c567d3832b7b5e51f8b",
                size: 144521448
            }
        } else {
            return None;
        };

    Some( package )
}

fn binaryen_package() -> Option< PrebuiltPackage > {
    let package =
        if cfg!( target_os = "linux" ) && cfg!( target_arch = "x86_64" ) {
            PrebuiltPackage {
                url: "https://github.com/koute/emscripten-build/releases/download/emscripten-1.37.26-1/binaryen-1.37.26-1-x86_64-unknown-linux-gnu.tgz",
                name: "binaryen",
                version: "1.37.26-1",
                arch: "x86_64-unknown-linux-gnu",
                hash: "9192a71b05abff031ec14574d51e40744c332a9307142b9279eb3544344b3cee",
                size: 12625187
            }
        } else if cfg!( target_os = "linux" ) && cfg!( target_arch = "x86" ) {
            PrebuiltPackage {
                url: "https://github.com/koute/emscripten-build/releases/download/emscripten-1.37.26-1/binaryen-1.37.26-1-i686-unknown-linux-gnu.tgz",
                name: "binaryen",
                version: "1.37.26-1",
                arch: "i686-unknown-linux-gnu",
                hash: "3b09c8d55308ae1dfd2e369b6a0dad361b53db0d39ae592020b353ffaf84f260",
                size: 12706588
            }
        } else {
            return None;
        };

    Some( package )
}

pub fn check_for_emcc( use_system_emscripten: bool, targeting_webasm: bool ) -> Option< PathBuf > {
    let emscripten_package =
        if use_system_emscripten {
            None
        } else {
            emscripten_package()
        };

    let binaryen_package =
        if use_system_emscripten || !targeting_webasm {
            None
        } else {
            binaryen_package()
        };

    if let Some( package ) = binaryen_package {
        let binaryen_path = download_package( &package );
        env::set_var( "BINARYEN", &binaryen_path.join( "binaryen" ) );
    }

    if let Some( package ) = emscripten_package {
        let emscripten_path = download_package( &package );
        let emscripten_bin_path = emscripten_path.join( "emscripten" );
        let emscripten_llvm_path = emscripten_path.join( "emscripten-fastcomp" );

        env::set_var( "EMSCRIPTEN", &emscripten_bin_path );
        env::set_var( "EMSCRIPTEN_FASTCOMP", &emscripten_llvm_path );
        env::set_var( "LLVM", &emscripten_llvm_path );

        return Some( emscripten_bin_path );
    }

    if check_if_command_exists( "emcc", None ) {
        return None;
    }

    if cfg!( any(linux) ) && Path::new( "/usr/lib/emscripten/emcc" ).exists() {
        if check_if_command_exists( "emcc", Some( "/usr/lib/emscripten" ) ) {
            // Arch package doesn't put Emscripten anywhere in the $PATH, but
            // it's there and it works.
            return Some( "/usr/lib/emscripten".into() );
        }
    } else if cfg!( any(windows) ) {
        if check_if_command_exists( "emcc.bat", None ) {
            return None;
        }
    }

    println_err!( "error: you don't have Emscripten installed!" );
    println_err!( "" );

    if Path::new( "/usr/bin/pacman" ).exists() {
        println_err!( "You can most likely install it like this:" );
        println_err!( "  sudo pacman -S emscripten" );
    } else if Path::new( "/usr/bin/apt-get" ).exists() {
        println_err!( "You can most likely install it like this:" );
        println_err!( "  sudo apt-get install emscripten" );
    } else if cfg!( target_os = "linux" ) {
        println_err!( "You can most likely find it in your distro's repositories." );
    } else if cfg!( target_os = "windows" ) {
        println_err!( "Download and install emscripten from the official site: http://kripken.github.io/emscripten-site/docs/getting_started/downloads.html" );
    }

    if cfg!( unix ) {
        if cfg!( target_os = "linux" ) {
            println_err!( "If not you can install it manually like this:" );
        } else {
            println_err!( "You can install it manually like this:" );
        }
        println_err!( "  curl -O https://s3.amazonaws.com/mozilla-games/emscripten/releases/emsdk-portable.tar.gz" );
        println_err!( "  tar -xzf emsdk-portable.tar.gz" );
        println_err!( "  source emsdk_portable/emsdk_env.sh" );
        println_err!( "  emsdk update" );
        println_err!( "  emsdk install sdk-incoming-64bit" );
        println_err!( "  emsdk activate sdk-incoming-64bit" );
    }

    exit( 101 );
}
