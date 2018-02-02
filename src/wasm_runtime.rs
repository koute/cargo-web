use std::path::Path;
use std::collections::BTreeMap;
use std::fmt::Write as FmtWrite;
use std::fmt::Display;

use unicode_categories::UnicodeCategories;
use handlebars::Handlebars;

use wasm_inline_js::JsSnippet;
use wasm_js_export::{JsExport, TypeMetadata};

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum RuntimeKind {
    Standalone,
    OnlyLoader
}

// This is probably a total overkill, but oh well.
fn to_js_identifier( string: &str ) -> String {
    // Source: https://mathiasbynens.be/notes/javascript-identifiers
    fn is_valid_starting_char( ch: char ) -> bool {
        ch.is_letter_uppercase() ||
        ch.is_letter_lowercase() ||
        ch.is_letter_titlecase() ||
        ch.is_letter_modifier() ||
        ch.is_letter_other() ||
        ch.is_number_letter()
    }

    fn replace_invalid_starting_char( ch: char ) -> char {
        if is_valid_starting_char( ch ) {
            ch
        } else {
            '_'
        }
    }

    fn is_valid_middle_char( ch: char ) -> bool {
        is_valid_starting_char( ch ) ||
        ch == '\u{200C}' ||
        ch == '\u{200D}' ||
        ch.is_mark_nonspacing() ||
        ch.is_mark_spacing_combining() ||
        ch.is_number_decimal_digit() ||
        ch.is_punctuation_connector()
    }

    fn replace_invalid_middle_char( ch: char ) -> char {
        if is_valid_middle_char( ch ) {
            ch
        } else {
            '_'
        }
    }

    string.chars().take( 1 ).map( replace_invalid_starting_char ).chain(
        string.chars().skip( 1 ).map( replace_invalid_middle_char )
    ).collect()
}

static LOADER_TEMPLATE: &str = include_str!( "wasm_runtime_loader.js" );
static STANDALONE_TEMPLATE: &str = include_str!( "wasm_runtime_standalone.js" );

fn join< T: Display, I: IntoIterator< Item = T > >( separator: &str, iter: I ) -> String {
    let mut output = String::new();
    for (index, item) in iter.into_iter().enumerate() {
        if index != 0 {
            write!( output, "{}", separator ).unwrap();
        }
        write!( output, "{}", item ).unwrap();
    }

    output
}

pub fn generate_js( runtime: RuntimeKind, main_symbol: Option< String >, wasm_path: &Path, prepend_js: &str, snippets: &[JsSnippet], exports: &[JsExport] ) -> String {
    let filename = wasm_path.file_name().unwrap().to_str().unwrap();
    let module_name = to_js_identifier( wasm_path.file_stem().unwrap().to_str().unwrap() );

    let mut snippets_js = String::new();
    for snippet in snippets {
        write!( snippets_js, "            \"{}\": function(", snippet.name ).unwrap();
        for nth in 0..snippet.arg_count() {
            if nth != 0 {
                write!( snippets_js, ", " ).unwrap();
            }
            write!( snippets_js, "${}", nth ).unwrap();
        }
        writeln!( snippets_js, ") {{" ).unwrap();

        let indent = "                ";
        let newline_indent = "\n                ";
        writeln!( snippets_js, "{}{}", indent, snippet.code.trim().replace( "\n", newline_indent ) ).unwrap();
        writeln!( snippets_js, "            }}," ).unwrap();
    }

    let mut exports_code = String::new();
    for export in exports {
        let mut code = String::new();

        let arg_names = join( ", ", export.metadata.args.iter().map( |arg| arg.name.as_str() ) );
        writeln!( code, "function {}({}) {{", export.metadata.name, arg_names ).unwrap();

        let arg_conversions = join( ", ", export.metadata.args.iter().map( |arg| {
            match arg.ty {
                TypeMetadata::I32 | TypeMetadata::F64 => format!( "{}", arg.name ),
                TypeMetadata::Custom { ref conversion_fn, .. } => format!( "{}({})", conversion_fn, arg.name )
            }
        }));

        let call = format!( "Module.instance.exports.{}({})", export.raw_name, arg_conversions );
        if let Some( ref result ) = export.metadata.result {
            match *result {
                TypeMetadata::I32 | TypeMetadata::F64 => {
                    writeln!( code, "    return {};", call ).unwrap();
                },
                TypeMetadata::Custom { ref conversion_fn, .. } => {
                    writeln!( code, "    return {}({});", conversion_fn, call ).unwrap();
                }
            }
        } else {
            writeln!( code, "    {};", call ).unwrap();
        }

        writeln!( code, "}}" ).unwrap();
        writeln!( exports_code,
            "                Module.exports.{} = {};",
            export.metadata.name, code
        ).unwrap();
    }

    let handlebars = Handlebars::new();
    let mut template_data = BTreeMap::new();
    template_data.insert( "wasm_filename", filename.to_owned() );
    template_data.insert( "module_name", module_name );
    template_data.insert( "snippets", snippets_js.trim().to_owned() );
    template_data.insert( "exports", exports_code.trim().to_owned() );
    template_data.insert( "prepend_js", prepend_js.to_owned() );

    if let Some( main_symbol ) = main_symbol {
        template_data.insert( "call_main", format!( "Module.instance.exports.{}();", main_symbol ) );
    } else {
        template_data.insert( "call_main", "".to_owned() );
    }

    let loader = handlebars.template_render( LOADER_TEMPLATE, &template_data ).unwrap();

    match runtime {
        RuntimeKind::Standalone => {
            template_data.insert( "loader", loader );
            handlebars.template_render( STANDALONE_TEMPLATE, &template_data ).unwrap()
        },
        RuntimeKind::OnlyLoader => {
            loader
        }
    }
}
