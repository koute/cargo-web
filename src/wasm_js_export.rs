use serde_json;
use base_x;

use wasm_context::{
    FunctionKind,
    Context
};

pub struct JsExport {
    pub raw_name: String,
    pub metadata: ExportMetadata
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum TypeMetadata {
    I32,
    F64,
    Custom {
        name: Option< String >,
        conversion_fn: String
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ArgMetadata {
    pub name: String,
    pub ty: TypeMetadata
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ExportMetadata {
    pub name: String,
    pub args: Vec< ArgMetadata >,
    pub result: Option< TypeMetadata >
}

// This is a base62 encoding which consists of only alpha-numeric characters.
// Generated with: (('A'..'Z').to_a + ('a'..'z').to_a + ('0'..'9').to_a).join("")
const ENCODING_BASE: &'static [u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";

const PREFIX: &'static str = "__JS_EXPORT_";

pub fn process( ctx: &mut Context ) -> Vec< JsExport > {
    let mut output = Vec::new();
    for function in ctx.functions.values_mut() {
        if let &mut FunctionKind::Definition { ref mut export, ref mut name, .. } = function {
            if export.names.len() == 1 && export.names[ 0 ].starts_with( PREFIX ) {
                let json_metadata = {
                    let encoded_metadata = &export.names[ 0 ][ PREFIX.len().. ];
                    base_x::decode( ENCODING_BASE, encoded_metadata )
                        .expect( "cannot decode `js_export!` symbol" )
                };

                let json_metadata = String::from_utf8( json_metadata )
                    .expect( "one of the `js_export!` symbols has metadata which is not valid UTF-8" );

                let metadata: ExportMetadata =
                    serde_json::from_str( &json_metadata )
                    .expect( "cannot parse `js_export!` symbol metadata" );

                export.names[ 0 ] = metadata.name.clone();
                if let &mut Some( ref mut name ) = name {
                    *name = metadata.name.clone();
                }

                output.push( JsExport {
                    metadata,
                    raw_name: export.names[ 0 ].clone()
                });
            }
        }
    }

    output
}
