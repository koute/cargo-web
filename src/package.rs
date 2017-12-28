use std::process::exit;
use std::path::PathBuf;
use std::io::{self, Read, Write};
use std::fs;

use app_dirs;
use pbr;
use sha2;
use reqwest::{
    header,
    Client,
    Url
};

use tempdir::TempDir;

use digest::Digest;

use utils::{
    read,
    write,
    unpack
};

const APP_INFO: app_dirs::AppInfo = app_dirs::AppInfo {
    name: "cargo-web",
    author: "Jan Bujak"
};

pub struct PrebuiltPackage {
    pub url: &'static str,
    pub name: &'static str,
    pub version: &'static str,
    pub arch: &'static str,
    pub hash: &'static str,
    pub size: u64,
}

pub fn download_package( package: &PrebuiltPackage ) -> PathBuf {
    let url = Url::parse( package.url ).unwrap();
    let package_filename = url.path_segments().unwrap().last().unwrap().to_owned();

    let unpack_path = app_dirs::app_dir( app_dirs::AppDataType::UserData, &APP_INFO, package.name )
        .unwrap()
        .join( package.arch );
    let version_path = unpack_path.join( ".version" );

    if let Ok( existing_version ) = read( &version_path ) {
        if existing_version == package.version {
            return unpack_path;
        }
    }

    if fs::metadata( &unpack_path ).is_ok() {
        fs::remove_dir_all( &unpack_path ).unwrap();
    }

    fs::create_dir_all( &unpack_path ).unwrap();

    println_err!( "Downloading {}...", package_filename );
    let client = Client::new();
    let mut response = client.get( url )
        .header( header::Connection::close() )
        .send()
        .unwrap();

    let tmpdir = TempDir::new( format!( "cargo-web-{}-download", package.name ).as_str() ).unwrap();
    let dlpath = tmpdir.path().join( &package_filename );
    let mut fp = fs::File::create( &dlpath ).unwrap();

    let length: Option< header::ContentLength > = response.headers().get().cloned();
    let length = length.map( |length| length.0 ).unwrap_or( package.size );
    let mut pb = pbr::ProgressBar::new( length );
    pb.set_units( pbr::Units::Bytes );

    let mut buffer = Vec::new();
    buffer.resize( 1024 * 1024, 0 );

    let mut hasher = sha2::Sha256::default();
    loop {
        let length = match response.read( &mut buffer ) {
            Ok( 0 ) => break,
            Ok( length ) => length,
            Err( ref err ) if err.kind() == io::ErrorKind::Interrupted => continue,
            Err( err ) => panic!( err )
        };

        let slice = &buffer[ 0..length ];
        hasher.input( slice );
        fp.write_all( slice ).unwrap();
        pb.add( length as u64 );
    }

    pb.finish();

    let actual_hash = hasher.result();
    let actual_hash = actual_hash.map( |byte| format!( "{:02x}", byte ) ).join( "" );

    if actual_hash != package.hash {
        println_err!( "error: the hash of {} doesn't match the expected hash!", package_filename );
        println_err!( "  actual: {}", actual_hash );
        println_err!( "  expected: {}", package.hash );
        exit( 101 );
    }

    println_err!( "Unpacking {}...", package_filename );
    unpack( &dlpath, &unpack_path ).unwrap();
    write( &version_path, package.version ).unwrap();

    println_err!( "Package {} was successfully installed!", package_filename );
    return unpack_path;
}
