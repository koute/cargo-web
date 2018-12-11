use std::path::Path;
use std::fs;

use sha1::Sha1;
use serde_json;

use wasm_context::{Context, FunctionKind};
use wasm_inline_js::JsSnippet;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub struct Snippet {
    pub name: String,
    pub code: String,
    pub arg_count: usize
}

fn hash( string: &str ) -> String {
    let mut hasher = Sha1::new();
    hasher.update( string.as_bytes() );
    format!( "{}", hasher.digest() )
}

pub fn process( target_dir: &Path, ctx: &Context ) -> Vec< JsSnippet > {
    let mut snippets = Vec::new();
    for (_, function) in &ctx.functions {
        if let &FunctionKind::Import { ref import, .. } = function {
            if import.module == "env" {
                let name_hash = hash( &import.field );
                let path = target_dir.join( ".cargo-web" ).join( "snippets" ).join( &name_hash[ 0..2 ] ).join( format!( "{}.json", name_hash ) );
                if path.exists() {
                    let blob = fs::read( path ).expect( "cannot read JS snippet from the filesystem" );
                    let snippet: Snippet = serde_json::from_slice( &blob ).expect( "corrupted JS snippet file" );
                    let snippet = JsSnippet {
                        name: snippet.name,
                        code: snippet.code,
                        arg_count: snippet.arg_count
                    };
                    snippets.push( snippet );
                }
            }
        }
    }

    snippets
}