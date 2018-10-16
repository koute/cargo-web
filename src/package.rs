use std::process::exit;
use std::path::PathBuf;
use std::io::{self, Read, Write};
use std::fs;
use std::env;

use pbr;
use sha2;
use reqwest::{
    header,
    Client,
    Proxy,
    Result as ReqResult,
    Url,
};

use tempdir::TempDir;

use digest::Digest;
use digest::generic_array::functional::FunctionalSequence;

use utils::{
    read,
    write,
    unpack
};

use app_info;

pub struct PrebuiltPackage {
    pub url: &'static str,
    pub name: &'static str,
    pub version: &'static str,
    pub arch: &'static str,
    pub hash: &'static str,
    pub size: u64,
}

// Creates a new client, supporting configuration from operating system variables if available.
fn create_client() -> ReqResult<Client> {
    let mut builder = Client::builder().danger_accept_invalid_certs( true );
    match env::var("HTTPS_PROXY") {
        Err(_) => {},
        Ok(proxy) => { builder = builder.proxy(Proxy::https(&proxy).unwrap()); }
    };
    match env::var("HTTP_PROXY") {
        Err(_) => {},
        Ok(proxy) => { builder = builder.proxy(Proxy::http(&proxy).unwrap()); }
    };
    match env::var("ALL_PROXY") {
        Err(_) => {},
        Ok(proxy) => { builder = builder.proxy(Proxy::all(&proxy).unwrap()); }
    };
    builder.build()
}

pub fn download_package( package: &PrebuiltPackage ) -> PathBuf {
    let url = Url::parse( package.url ).unwrap();
    let package_filename = url.path_segments().unwrap().last().unwrap().to_owned();

    let unpack_path = app_info::app_dir( app_info::AppDataType::UserData, &app_info::APP_INFO, package.name )
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

    eprintln!( "Downloading {}...", package_filename );
    let client = create_client().unwrap();
    let mut response = client.get( url )
        .header( header::CONNECTION, "close" )
        .send()
        .unwrap();

    let tmpdir = TempDir::new( format!( "cargo-web-{}-download", package.name ).as_str() ).unwrap();
    let dlpath = tmpdir.path().join( &package_filename );
    let mut fp = fs::File::create( &dlpath ).unwrap();

    let length = response.headers().get( header::CONTENT_LENGTH )
        .and_then( |len| len.to_str().ok() )
        .and_then( |len| len.parse().ok() )
        .unwrap_or( package.size );
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
        eprintln!( "error: the hash of {} doesn't match the expected hash!", package_filename );
        eprintln!( "  actual: {}", actual_hash );
        eprintln!( "  expected: {}", package.hash );
        exit( 101 );
    }

    eprintln!( "Unpacking {}...", package_filename );
    unpack( &dlpath, &unpack_path ).unwrap();
    write( &version_path, package.version ).unwrap();

    eprintln!( "Package {} was successfully installed!", package_filename );
    return unpack_path;
}
