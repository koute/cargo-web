use std::path::{Path, PathBuf};
use std::fs::{self, File};
use std::io::{self, Read, Write};

use parity_wasm;
use cargo_shim::BuildConfig;
use serde_json;

use wasm_gc;

use wasm_context::Context;
use wasm_inline_js;
use wasm_export_main;
use wasm_export_table;
use wasm_hook_grow;
use wasm_intrinsics;
use wasm_runtime::{self, RuntimeKind};
use wasm_js_export;
use wasm_js_snippet;
use utils::get_sha1sum;

#[derive(Serialize, Deserialize)]
struct Metadata {
    wasm_hash: String
}

pub fn process_wasm_file< P: AsRef< Path > + ?Sized >( uses_old_stdweb: bool, runtime: RuntimeKind, build: &BuildConfig, prepend_js: &str, target_dir: &Path, artifact: &P ) -> Option< PathBuf > {
    if !build.triplet.as_ref().map( |triplet| triplet == "wasm32-unknown-unknown" ).unwrap_or( false ) {
        return None;
    }

    let path = artifact.as_ref();
    if !path.extension().map( |ext| ext == "wasm" ).unwrap_or( false ) {
        return None;
    }

    if !uses_old_stdweb {
        new_process_wasm_file( runtime, prepend_js, target_dir, path )
    } else {
        old_process_wasm_file( runtime, prepend_js, path )
    }
}

fn new_process_wasm_file( runtime: RuntimeKind, prepend_js: &str, target_dir: &Path, path: &Path ) -> Option< PathBuf > {
    eprintln!( "    Processing {:?}...", path.file_name().unwrap() );

    let mut module = parity_wasm::deserialize_file( &path ).unwrap();
    let mut ctx = Context::from_module( module );
    let snippets = wasm_js_snippet::process( target_dir, &mut ctx );
    let intrinsics = wasm_intrinsics::process( &mut ctx );
    let main_symbol = wasm_export_main::process( &mut ctx );
    let exports = wasm_js_export::process( &mut ctx );
    wasm_export_main::process( &mut ctx );
    wasm_export_table::process( &mut ctx );
    wasm_hook_grow::process( &mut ctx );
    module = ctx.into_module();

    // TODO: Remove this once we stop losing information when we process the `.wasm` file.
    //       (That is - migrate the `#[js_export]` macro to use another mechanism.)
    let _ = fs::remove_file( path );

    parity_wasm::serialize_to_file( path, module ).unwrap();

    let mut all_snippets: Vec< _ > = snippets.into_iter().chain( intrinsics.into_iter() ).collect();
    all_snippets.sort_by( |a, b| a.name.cmp( &b.name ) );

    let js_path = path.with_extension( "js" );
    let js = wasm_runtime::generate_js( runtime, main_symbol, path, prepend_js, &all_snippets, &exports );
    let mut fp = File::create( &js_path ).unwrap();
    fp.write_all( js.as_bytes() ).unwrap();

    eprintln!( "    Finished processing of {:?}!", path.file_name().unwrap() );
    Some( js_path )

}

fn old_process_wasm_file( runtime: RuntimeKind, prepend_js: &str, path: &Path ) -> Option< PathBuf > {
    let wasm_hash = get_sha1sum( path ).expect( "cannot calculate sha1sum of the `.wasm` file" );
    debug!( "Hash of {:?}: {}", path, wasm_hash );

    let js_path = path.with_extension( "js" );
    let metadata_path = path.with_extension( "cargoweb-metadata" );
    if js_path.exists() && metadata_path.exists() {
        // TODO: This is just a quick workaround. We should always regenerate the `.js` file.
        let fp = File::open( &metadata_path ).expect( "cannot open the metadata file" );
        let metadata: Metadata = serde_json::from_reader( fp ).expect( "cannot deserialize metadata; delete your `target` directory" );
        if metadata.wasm_hash == wasm_hash {
            debug!( "Skipping `.js` generation and `.wasm` processing!" );
            return Some( js_path );
        }
    }

    eprintln!( "    Garbage collecting {:?}...", path.file_name().unwrap() );
    wasm_gc::run( &path, &path );

    eprintln!( "    Processing {:?}...", path.file_name().unwrap() );
    let mut module = parity_wasm::deserialize_file( &path ).unwrap();
    let mut ctx = Context::from_module( module );
    let snippets = wasm_inline_js::process_and_extract( &mut ctx );
    let intrinsics = wasm_intrinsics::process( &mut ctx );
    let main_symbol = wasm_export_main::process( &mut ctx );
    let exports = wasm_js_export::process( &mut ctx );
    wasm_export_main::process( &mut ctx );
    wasm_export_table::process( &mut ctx );
    wasm_hook_grow::process( &mut ctx );
    module = ctx.into_module();

    // At least on Linux when a `.wasm` file is built it's
    // hard-linked from two places:
    //    1) target/wasm32-unknown-unknown/release/$name.wasm
    //    2) target/wasm32-unknown-unknown/release/deps/$name.wasm
    //
    // If you trigger a `cargo build` in a case where your project
    // doesn't need to be rebuilt it will just recreate
    // the `deps/$name.wasm` -> `$name.wasm` hard-link
    // and report that the file was rebuilt. (Even though it wasn't!)
    //
    // This wouldn't normally be a problem, however since we
    // modify the `.wasm` file we end up modifying *both* of
    // them which breaks any subsequent builds.
    //
    // So we forcefully remove the `$name.wasm` here before
    // overwriting it to get rid of the hard-link.
    let _ = fs::remove_file( path );

    parity_wasm::serialize_to_file( path, module ).unwrap();

    let mut all_snippets: Vec< _ > = snippets.into_iter().chain( intrinsics.into_iter() ).collect();

    all_snippets.sort_by( |a, b| a.name.cmp( &b.name ) );

    let js = wasm_runtime::generate_js( runtime, main_symbol, path, prepend_js, &all_snippets, &exports );
    let mut fp = File::create( &js_path ).unwrap();
    fp.write_all( js.as_bytes() ).unwrap();

    let new_wasm_hash = get_sha1sum( path ).expect( "cannot calculate sha1sum of the `.wasm` file" );
    debug!( "New hash of {:?}: {}", path, new_wasm_hash );

    let fp = File::create( &metadata_path ).unwrap();
    serde_json::to_writer( fp, &Metadata { wasm_hash: new_wasm_hash } ).unwrap();

    eprintln!( "    Finished processing of {:?}!", path.file_name().unwrap() );
    Some( js_path )
}
