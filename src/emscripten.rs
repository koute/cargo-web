use std::process::exit;
use std::path::{Path, PathBuf};

use package::{
    PrebuiltPackage,
    download_package
};
use utils::check_if_command_exists;

fn emscripten_package() -> Option< PrebuiltPackage > {
    let package =
        if cfg!( target_os = "linux" ) && cfg!( target_arch = "x86_64" ) {
            PrebuiltPackage {
                url: "https://github.com/koute/emscripten-build/releases/download/emscripten-1.37.27-1/emscripten-1.37.27-1-x86_64-unknown-linux-gnu.tgz",
                name: "emscripten",
                version: "1.37.27-1",
                arch: "x86_64-unknown-linux-gnu",
                hash: "43e653d26bfe95b010267538949e2d0cb23364571972042165d0258d55e8ca66",
                size: 136902444
            }
        } else if cfg!( target_os = "linux" ) && cfg!( target_arch = "x86" ) {
            PrebuiltPackage {
                url: "https://github.com/koute/emscripten-build/releases/download/emscripten-1.37.27-1/emscripten-1.37.27-1-i686-unknown-linux-gnu.tgz",
                name: "emscripten",
                version: "1.37.27-1",
                arch: "i686-unknown-linux-gnu",
                hash: "a3a1e4622f4509b903eaf76c3b1c7fc981f656185f3fcb7cd8a81718d0e11bb3",
                size: 144527242
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
                url: "https://github.com/koute/emscripten-build/releases/download/emscripten-1.37.27-1/binaryen-1.37.27-1-x86_64-unknown-linux-gnu.tgz",
                name: "binaryen",
                version: "1.37.27-1",
                arch: "x86_64-unknown-linux-gnu",
                hash: "aa46c2d3d6031481a88c45e072acb1c625fbc22aae8a5271fd70f5b879666c1a",
                size: 12625100
            }
        } else if cfg!( target_os = "linux" ) && cfg!( target_arch = "x86" ) {
            PrebuiltPackage {
                url: "https://github.com/koute/emscripten-build/releases/download/emscripten-1.37.27-1/binaryen-1.37.27-1-i686-unknown-linux-gnu.tgz",
                name: "binaryen",
                version: "1.37.27-1",
                arch: "i686-unknown-linux-gnu",
                hash: "2a3eff1a7bbb5f5e4bceb0da1ebd508c6458d1dfe7f511641668db4d96b98d8a",
                size: 12706642
            }
        } else {
            return None;
        };

    Some( package )
}

fn check_emscripten() {
    let binary = if cfg!( windows ) {
        "emcc.bat"
    } else {
        "emcc"
    };

    if check_if_command_exists( binary, None ) {
        return;
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

pub struct Emscripten {
    pub binaryen_path: Option< PathBuf >,
    pub emscripten_path: PathBuf,
    pub emscripten_llvm_path: PathBuf
}

pub fn initialize_emscripten(
    use_system_emscripten: bool,
    targeting_webasm: bool
) -> Option< Emscripten > {

    if use_system_emscripten {
        check_emscripten();
        return None;
    }

    let emscripten_package = match emscripten_package() {
        Some( pkg ) => pkg,
        None => {
            check_emscripten();
            return None;
        }
    };

    let binaryen_package = if targeting_webasm {
        match binaryen_package() {
            Some( pkg ) => Some( pkg ),
            None => {
                check_emscripten();
                return None;
            }
        }
    } else {
        None
    };


    let emscripten_root = download_package( &emscripten_package );
    let emscripten_path = emscripten_root.join( "emscripten" );
    let emscripten_llvm_path = emscripten_root.join( "emscripten-fastcomp" );
    let binaryen_path = if let Some( binaryen_package ) = binaryen_package {
        let binaryen_root = download_package( &binaryen_package );
        Some( binaryen_root.join( "binaryen" ) )
    } else {
        None
    };

    Some( Emscripten {
        binaryen_path,
        emscripten_path,
        emscripten_llvm_path
    })
}
