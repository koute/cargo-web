use std::path::Path;
use std::collections::BTreeMap;
use std::fmt::Write as FmtWrite;

use unicode_categories::UnicodeCategories;
use handlebars::Handlebars;

use wasm_inline_js::JsSnippet;

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

static RUNTIME_TEMPLATE: &str = include_str!( "wasm_runtime.js" );

pub fn generate_js( wasm_path: &Path, snippets: &[JsSnippet] ) -> String {
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

    let handlebars = Handlebars::new();
    let mut template_data = BTreeMap::new();
    template_data.insert( "wasm_filename", filename.to_owned() );
    template_data.insert( "module_name", module_name );
    template_data.insert( "snippets", snippets_js.trim().to_owned() );
    let output = handlebars.template_render( RUNTIME_TEMPLATE, &template_data ).unwrap();

    output
}
