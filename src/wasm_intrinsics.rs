use std::collections::HashMap;

use wasm_inline_js::JsSnippet;
use wasm_context::{
    FnTy,
    FunctionKind,
    Context,
    ValueType
};

use self::ValueType::{F32, F64};

const INTRINSICS: &'static [(&'static str, &'static [ValueType], Option< ValueType >, &'static str)] = &[
    ("sin", &[F64], Some( F64 ), "return Math.sin( $0 );"),
    ("cos", &[F64], Some( F64 ), "return Math.cos( $0 );"),
    ("exp", &[F64], Some( F64 ), "return Math.exp( $0 );"),
    ("exp2", &[F64], Some( F64 ), "return Math.pow( 2, $0 );"),
    ("log", &[F64], Some( F64 ), "return Math.log( $0 );"),
    ("log2", &[F64], Some( F64 ), "return Math.log2( $0 );"),
    ("log10", &[F64], Some( F64 ), "return Math.log10( $0 );"),
    ("round", &[F64], Some( F64 ), "return Math.round( $0 );"),

    ("Math_tan", &[F64], Some( F64 ), "return Math.tan( $0 );"),
    ("Math_sinh", &[F64], Some( F64 ), "return Math.sinh( $0 );"),
    ("Math_cosh", &[F64], Some( F64 ), "return Math.cosh( $0 );"),
    ("Math_tanh", &[F64], Some( F64 ), "return Math.tanh( $0 );"),
    ("Math_asin", &[F64], Some( F64 ), "return Math.asin( $0 );"),
    ("Math_acos", &[F64], Some( F64 ), "return Math.acos( $0 );"),
    ("Math_atan", &[F64], Some( F64 ), "return Math.atan( $0 );"),
    ("Math_cbrt", &[F64], Some( F64 ), "return Math.cbrt( $0 );"),
    ("Math_log1p", &[F64], Some( F64 ), "return Math.log1p( $0 );"),

    ("Math_atan2", &[F64, F64], Some( F64 ), "return Math.atan( $0, $1 );"),
    ("Math_hypot", &[F64, F64], Some( F64 ), "return Math.hypot( $0, $1 );"),
    ("fmod", &[F64, F64], Some( F64 ), "return $0 % $1;"),
    ("pow", &[F64, F64], Some( F64 ), "return Math.pow( $0, $1 );"),

    ("sinf", &[F32], Some( F32 ), "return Math.sin( $0 );"),
    ("cosf", &[F32], Some( F32 ), "return Math.cos( $0 );"),
    ("expf", &[F32], Some( F32 ), "return Math.exp( $0 );"),
    ("exp2f", &[F32], Some( F32 ), "return Math.pow( 2, $0 );"),
    ("logf", &[F32], Some( F32 ), "return Math.log( $0 );"),
    ("log2f", &[F32], Some( F32 ), "return Math.log2( $0 );"),
    ("log10f", &[F32], Some( F32 ), "return Math.log10( $0 );"),
    ("roundf", &[F32], Some( F32 ), "return Math.round( $0 );"),

    ("fmodf", &[F32, F32], Some( F32 ), "return $0 % $1;"),
    ("powf", &[F32, F32], Some( F32 ), "return Math.pow( $0, $1 );"),
];

pub fn process( ctx: &mut Context ) -> Vec< JsSnippet > {
    let mut snippets = Vec::new();
    let intrinsics: HashMap< _, _ > = INTRINSICS.iter().map( |&(name, args, return_ty, code)| {
        (name, (args, return_ty, code))
    }).collect();

    for function in ctx.functions.values() {
        match function {
            &FunctionKind::Import { ref import, .. } if import.module == "env" => {
                if let Some( &(args, return_type, code) ) = intrinsics.get( import.field.as_str() ) {
                    snippets.push( JsSnippet {
                        name: import.field.clone(),
                        code: code.to_owned(),
                        ty: FnTy {
                            params: args.iter().cloned().collect(),
                            return_type
                        }
                    });
                }
            },
            _ => {}
        }
    }

    snippets
}
