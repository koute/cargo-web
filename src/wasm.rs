use std::path::{Path, PathBuf};
use std::fs::{self, File};
use std::io::Write;

use parity_wasm;
use cargo_shim::BuildConfig;

use wasm_gc;

use wasm_context::Context;
use wasm_inline_js;
use wasm_export_main;
use wasm_export_table;
use wasm_hook_grow;
use wasm_intrinsics;
use wasm_runtime;

pub fn process_wasm_files< P: AsRef< Path > >( build: &BuildConfig, artifacts: &[P] ) -> Vec< PathBuf > {
    if !build.triplet.as_ref().map( |triplet| triplet == "wasm32-unknown-unknown" ).unwrap_or( false ) {
        return Vec::new();
    }

    let mut outputs = Vec::new();
    for artifact in artifacts {
        let path = artifact.as_ref();
        if !path.extension().map( |ext| ext == "wasm" ).unwrap_or( false ) {
            continue;
        }

        let js_path = path.with_extension( "js" );
        outputs.push( js_path.clone() );

        if js_path.exists() {
            let js_mtime = fs::metadata( &js_path ).unwrap().modified().unwrap();
            let wasm_mtime = fs::metadata( path ).unwrap().modified().unwrap();
            if js_mtime >= wasm_mtime {
                // We've already ran; nothing to do here.
                continue;
            }
        }

        println_err!( "    Garbage collecting {:?}...", path.file_name().unwrap() );
        wasm_gc::run( &path, &path );

        println_err!( "    Processing {:?}...", path.file_name().unwrap() );
        let mut module = parity_wasm::deserialize_file( &path ).unwrap();
        let mut ctx = Context::from_module( module );
        let snippets = wasm_inline_js::process_and_extract( &mut ctx );
        let intrinsics = wasm_intrinsics::process( &mut ctx );
        wasm_export_main::process( &mut ctx );
        wasm_export_table::process( &mut ctx );
        wasm_hook_grow::process( &mut ctx );
        module = ctx.into_module();

        parity_wasm::serialize_to_file( path, module ).unwrap();

        let all_snippets: Vec< _ > = snippets.into_iter().chain( intrinsics.into_iter() ).collect();
        let js = wasm_runtime::generate_js( path, &all_snippets );
        let mut fp = File::create( js_path ).unwrap();
        fp.write_all( js.as_bytes() ).unwrap();

        println_err!( "    Finished processing of {:?}!", path.file_name().unwrap() );
    }

    outputs
}
