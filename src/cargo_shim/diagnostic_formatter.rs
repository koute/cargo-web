use std::fmt::{self, Write};
use ansi_term::{Color, Style};
use serde_json;
use super::cargo_output::Message;
use super::rustc_diagnostic::Diagnostic;

struct Pad< T: fmt::Display >( usize, T );

impl< T: fmt::Display > fmt::Display for Pad< T > {
    // This could be made more efficient, but I guess who cares?
    fn fmt( &self, fmt: &mut fmt::Formatter ) -> fmt::Result {
        let output = format!( "{}", self.1 );
        let mut width = output.len();

        write!( fmt, "{}", output )?;
        while width < self.0 {
            write!( fmt, " " )?;
            width += 1;
        }

        Ok(())
    }
}

struct Spaces( usize );

impl fmt::Display for Spaces {
    fn fmt( &self, fmt: &mut fmt::Formatter ) -> fmt::Result {
        for _ in 0..self.0 {
            write!( fmt, " " )?;
        }

        Ok(())
    }
}

struct MaybePrint< T: fmt::Display >( bool, T );

impl< T: fmt::Display > fmt::Display for MaybePrint< T > {
    fn fmt( &self, fmt: &mut fmt::Formatter ) -> fmt::Result {
        if self.0 {
            write!( fmt, "{}", self.1 )?;
        }

        Ok(())
    }
}

struct Repeat< T: fmt::Display >( usize, T );

impl< T: fmt::Display > fmt::Display for Repeat< T > {
    fn fmt( &self, fmt: &mut fmt::Formatter ) -> fmt::Result {
        for _ in 0..self.0 {
            write!( fmt, "{}", self.1 )?;
        }

        Ok(())
    }
}

fn level_color( level: &str ) -> Style {
    match level {
        "error" => Color::Red,
        "warning" => Color::Yellow,
        "note" => Color::White,
        "help" => Color::Blue,
        _ => Color::Red
    }.bold()
}

fn print_diagnostic< W: Write >( use_color: bool, diag: &Diagnostic, fp: &mut W ) -> fmt::Result {
    let color = level_color( diag.level.as_str() );
    let arrow_color = Color::Blue.bold();

    write!( fp, "{}{}",
        MaybePrint( use_color, color.prefix() ),
        diag.level
    )?;
    if let Some( ref code ) = diag.code {
        if code.code.starts_with( "E" ) {
            write!( fp, "[{}]", code.code )?;
        }
    };

    writeln!( fp, "{}: {}", MaybePrint( use_color, color.suffix() ), diag.message )?;
    let line_number_digits = diag.spans.last().map( |span| format!( "{}", span.line_start + span.text.iter().count() ).len() ).unwrap_or( 0 );
    for span in &diag.spans {
        writeln!( fp, "{}{}-->{} {}:{}:{}",
            Spaces( line_number_digits ),
            MaybePrint( use_color, arrow_color.prefix() ),
            MaybePrint( use_color, arrow_color.suffix() ),
            span.file_name,
            span.line_start,
            span.column_start
        )?;

        writeln!( fp, "{}{} |{}",
            MaybePrint( use_color, arrow_color.prefix() ),
            Spaces( line_number_digits ),
            MaybePrint( use_color, arrow_color.suffix() )
        )?;

        let line_count = span.text.len();
        let is_multiline = line_count > 1;
        let mut skipped = false;
        let mut nth_line = span.line_start;
        for line in &span.text {
            let at_start = nth_line - span.line_start < 4;
            let at_end = span.line_end - nth_line < 2;

            if !at_start && !at_end {
                nth_line += 1;
                if skipped {
                    continue;
                }
                skipped = true;
                writeln!( fp, "{arrow_color_s}{dots}{arrow_color_e}  {color_s}|{color_e}",
                    arrow_color_s = MaybePrint( use_color, arrow_color.prefix() ),
                    arrow_color_e = MaybePrint( use_color, arrow_color.suffix() ),
                    dots = Repeat( line_number_digits + 1, '.' ),
                    color_s = MaybePrint( use_color, color.prefix() ),
                    color_e = MaybePrint( use_color, color.suffix() ),
                )?;
                continue;
            }

            writeln!( fp, "{arrow_color_s}{line_num} |{arrow_color_e} {color_s}{multiline_start}{color_e}{text}",
                arrow_color_s = MaybePrint( use_color, arrow_color.prefix() ),
                arrow_color_e = MaybePrint( use_color, arrow_color.suffix() ),
                line_num = Pad( line_number_digits, nth_line ),
                text = line.text,

                color_s = MaybePrint( use_color && is_multiline, color.prefix() ),
                color_e = MaybePrint( use_color && is_multiline, color.suffix() ),
                multiline_start = MaybePrint(
                    is_multiline,
                    if nth_line == span.line_start {
                        "/ "
                    } else {
                        "| "
                    }
                )
            )?;
            nth_line += 1;
        }
        if let Some( last_line ) = span.text.last() {
            write!( fp, "{arrow_color_s}{spaces} |{arrow_color_e}",
                arrow_color_s = MaybePrint( use_color, arrow_color.prefix() ),
                spaces = Spaces( line_number_digits ),
                arrow_color_e = MaybePrint( use_color, arrow_color.suffix() ),
            )?;
            for _ in 0..last_line.highlight_start {
                write!( fp, " " )?;
            }
            write!( fp, "{}{}",
                MaybePrint( use_color, color.prefix() ),
                MaybePrint( is_multiline, "|_" )
            )?;
            for _ in last_line.highlight_start..last_line.highlight_end - 1 {
                if is_multiline {
                    write!( fp, "_" )?;
                } else {
                    write!( fp, "^" )?;
                }
            }
            write!( fp, "^" )?;

            if let Some( ref label ) = span.label {
                write!( fp, " {}", label )?;
            }
            write!( fp, "{}", MaybePrint( use_color, color.suffix() ) )?;
            writeln!( fp, "" )?;
        }
    }

    for child in &diag.children {
        writeln!( fp, "{}{} |{}",
            MaybePrint( use_color, arrow_color.prefix() ),
            Spaces( line_number_digits ),
            MaybePrint( use_color, arrow_color.suffix() )
        )?;

        let color = level_color( child.level.as_str() );
        writeln!( fp, "{}{}= {}{}{}: {}{}",
            MaybePrint( use_color, arrow_color.prefix() ),
            Spaces( line_number_digits + 1 ),
            MaybePrint( use_color, arrow_color.suffix() ),
            MaybePrint( use_color, color.prefix() ),
            child.level,
            child.message,
            MaybePrint( use_color, color.suffix() )
        )?;
    }

    Ok(())
}

fn color_header( output: &mut String, header: &str, line: &str ) -> Result< bool, fmt::Error > {
    if !line.starts_with( header ) {
        return Ok( false );
    }

    let index = match line.char_indices().skip_while( |&(_, ch)| ch != ':' ).next().map( |(index, _)| index ) {
        Some( index ) => index,
        None => return Ok( false )
    };

    let color = level_color( header );
    writeln!(
        output,
        "{}{}{}:{}",
        color.prefix(),
        &line[ ..index ],
        color.suffix(),
        &line[ index + 1.. ]
    )?;
    Ok( true )
}

fn skip_spaces( p: &mut &str ) {
    while p.chars().next() == Some( ' ' ) {
        *p = &p[ 1.. ];
    }
}

fn skip_pattern( p: &mut &str, pattern: &str ) -> bool {
    if p.starts_with( pattern ) {
        *p = &p[ pattern.len().. ];
        true
    } else {
        false
    }
}

fn skip_numbers( p: &mut &str ) {
    while p.chars().next().map( |ch| ch >= '0' && ch <= '9' ).unwrap_or( false ) {
        *p = &p[ 1.. ];
    }
}

fn color_arrow( output: &mut String, line: &str ) -> Result< bool, fmt::Error > {
    let mut p = line;
    skip_spaces( &mut p );
    if skip_pattern( &mut p, "-->" ) {
        let color = Color::Blue.bold();
        let split_at = line.len() - p.len();
        writeln!(
            output,
            "{}{}{}{}",
            color.prefix(),
            &line[ 0..split_at ],
            color.suffix(),
            &line[ split_at.. ]
        )?;
        return Ok( true );
    }

    Ok( false )
}

fn color_line_number_column( output: &mut String, line: &str ) -> Result< bool, fmt::Error > {
    let mut p = line;
    skip_spaces( &mut p );
    skip_numbers( &mut p );
    skip_spaces( &mut p );
    if skip_pattern( &mut p, "|" ) {
        let color = Color::Blue.bold();
        let split_at = line.len() - p.len();
        writeln!(
            output,
            "{}{}{}{}",
            color.prefix(),
            &line[ 0..split_at ],
            color.suffix(),
            &line[ split_at.. ]
        )?;
        return Ok( true );
    }
    Ok( false )
}

fn simple_coloring( message: &str ) -> Result< String, fmt::Error > {
    if message.is_empty() {
        return Ok( String::new() );
    }

    let mut output = String::new();
    for line in message.lines() {
        if color_header( &mut output, "note", line )? {
            continue;
        }
        if color_header( &mut output, "warning", line )? {
            continue;
        }
        if color_header( &mut output, "error", line )? {
            continue;
        }
        if color_arrow( &mut output, line )? {
            continue;
        }
        if color_line_number_column( &mut output, line )? {
            continue;
        }

        output.push_str( line );
        output.push_str( "\n" );
    }

    if !message.ends_with( "\n" ) {
        output.pop();
    }

    Ok( output )
}

pub fn print( use_color: bool, message: &Message ) {
    let diag = &message.message;

    // Here we get the human readable message from rustc;
    // unfortunately it's without color. ):
    if let Some( ref original ) = diag.rendered {
        let mut generated = String::new();
        print_diagnostic( false, diag, &mut generated ).unwrap();

        if original.trim() != generated.trim() {
            // We printed the message differently than how rustc would do it.

            if cfg!( feature = "development-mode" ) {
                // Automatically generate a testcase so we can fix it.
                let json = serde_json::to_string_pretty( &diag ).unwrap();
                println!( "#[cfg(test)]" );
                println!( "static TEST_N_JSON: &'static str = r##\"" );
                println!( "{}", json.trim() );
                println!( "\"##;" );
                println!( "#[cfg(test)]" );
                println!( "static TEST_N_EXPECTED: &'static str = r##\"" );
                println!( "{}", original.trim() );
                println!( "\"##;" );
                println!( "#[test]" );
                println!( "fn test_n() {{" );
                println!( "    test_message_printing( TEST_N_JSON, TEST_N_EXPECTED );" );
                println!( "}}" );
                panic!( "We can't property print this!" );
            } else {
                // Just give up and print out a message colored with heuristics.
                if use_color {
                    eprint!( "{}", simple_coloring( original ).expect( "coloring failed" ) );
                } else {
                    eprint!( "{}", original );
                }

                return;
            }
        }
    }

    // Our message is the same as rustc's, so let's output it,
    // but with color!
    //
    // It's really silly that I have to resort to this,
    // but as far as I know there is no other way to get
    // colorized messages out of rustc when using the JSON
    // message format.
    let mut output = String::new();
    print_diagnostic( cfg!( unix ) && use_color, diag, &mut output ).unwrap();
    eprint!( "{}", output );
}

#[cfg(test)]
fn test_message_printing( json: &str, expected: &str ) {
    // We need this to get nice multiline output,
    // since `Debug` for strings prints them all
    // on a single line.
    #[derive(PartialEq)]
    struct DisplayDebug< T >( T );
    impl< T: fmt::Display > fmt::Debug for DisplayDebug< T > {
        fn fmt( &self, fmt: &mut fmt::Formatter ) -> fmt::Result {
            write!( fmt, "\n{}\n", self.0 )
        }
    }

    let diag: Diagnostic = serde_json::from_str( &json ).unwrap();
    let mut generated = String::new();
    print_diagnostic( false, &diag, &mut generated ).unwrap();
    assert_eq!( DisplayDebug( generated.trim() ), DisplayDebug( expected.trim() ) );
}

#[cfg(test)]
static TEST_BASIC_ERROR_JSON: &'static str = r##"
{
  "message": "cannot find value `foobar` in this scope",
  "code": {
    "code": "E0425",
    "explanation": "\n"
  },
  "level": "error",
  "spans": [
    {
      "file_name": "src/main.rs",
      "byte_start": 47,
      "byte_end": 53,
      "line_start": 3,
      "line_end": 3,
      "column_start": 5,
      "column_end": 11,
      "is_primary": true,
      "text": [
        {
          "text": "    foobar",
          "highlight_start": 5,
          "highlight_end": 11
        }
      ],
      "label": "not found in this scope",
      "suggested_replacement": null,
      "expansion": null
    }
  ],
  "children": [],
  "rendered": "error[E0425]: cannot find value `foobar` in this scope\n --> src/main.rs:3:5\n  |\n3 |     foobar\n  |     ^^^^^^ not found in this scope\n\n"
}
"##;
#[cfg(test)]
static TEST_BASIC_ERROR_EXPECTED: &'static str = r##"
error[E0425]: cannot find value `foobar` in this scope
 --> src/main.rs:3:5
  |
3 |     foobar
  |     ^^^^^^ not found in this scope
"##;
#[test]
fn test_basic_error() {
    test_message_printing( TEST_BASIC_ERROR_JSON, TEST_BASIC_ERROR_EXPECTED );
}

#[cfg(test)]
static TEST_BASIC_MULTILINE_JSON: &'static str = r##"
{
  "message": "constant item is never used: `FOOBAR`",
  "code": {
    "code": "dead_code",
    "explanation": null
  },
  "level": "warning",
  "spans": [
    {
      "file_name": "src/main.rs",
      "byte_start": 0,
      "byte_end": 34,
      "line_start": 1,
      "line_end": 2,
      "column_start": 1,
      "column_end": 3,
      "is_primary": true,
      "text": [
        {
          "text": "const FOOBAR: &'static str = r\"",
          "highlight_start": 1,
          "highlight_end": 32
        },
        {
          "text": "\";",
          "highlight_start": 1,
          "highlight_end": 3
        }
      ],
      "label": null,
      "suggested_replacement": null,
      "expansion": null
    }
  ],
  "children": [
    {
      "message": "#[warn(dead_code)] on by default",
      "code": null,
      "level": "note",
      "spans": [],
      "children": [],
      "rendered": null
    }
  ],
  "rendered": "warning: constant item is never used: `FOOBAR`\n --> src/main.rs:1:1\n  |\n1 | / const FOOBAR: &'static str = r\"\n2 | | \";\n  | |__^\n  |\n  = note: #[warn(dead_code)] on by default\n\n"
}
"##;
#[cfg(test)]
static TEST_BASIC_MULTILINE_EXPECTED: &'static str = r##"
warning: constant item is never used: `FOOBAR`
 --> src/main.rs:1:1
  |
1 | / const FOOBAR: &'static str = r"
2 | | ";
  | |__^
  |
  = note: #[warn(dead_code)] on by default
"##;
#[test]
fn test_basic_multiline() {
    test_message_printing( TEST_BASIC_MULTILINE_JSON, TEST_BASIC_MULTILINE_EXPECTED );
}

#[cfg(test)]
static TEST_LONG_MULTILINE_JSON: &'static str = r##"
{
  "message": "constant item is never used: `FOOBAR`",
  "code": {
    "code": "dead_code",
    "explanation": null
  },
  "level": "warning",
  "spans": [
    {
      "file_name": "src/main.rs",
      "byte_start": 0,
      "byte_end": 54,
      "line_start": 1,
      "line_end": 12,
      "column_start": 1,
      "column_end": 3,
      "is_primary": true,
      "text": [
        {
          "text": "const FOOBAR: &'static str = r\"",
          "highlight_start": 1,
          "highlight_end": 32
        },
        {
          "text": "A",
          "highlight_start": 1,
          "highlight_end": 2
        },
        {
          "text": "B",
          "highlight_start": 1,
          "highlight_end": 2
        },
        {
          "text": "C",
          "highlight_start": 1,
          "highlight_end": 2
        },
        {
          "text": "D",
          "highlight_start": 1,
          "highlight_end": 2
        },
        {
          "text": "E",
          "highlight_start": 1,
          "highlight_end": 2
        },
        {
          "text": "F",
          "highlight_start": 1,
          "highlight_end": 2
        },
        {
          "text": "G",
          "highlight_start": 1,
          "highlight_end": 2
        },
        {
          "text": "H",
          "highlight_start": 1,
          "highlight_end": 2
        },
        {
          "text": "I",
          "highlight_start": 1,
          "highlight_end": 2
        },
        {
          "text": "J",
          "highlight_start": 1,
          "highlight_end": 2
        },
        {
          "text": "\";",
          "highlight_start": 1,
          "highlight_end": 3
        }
      ],
      "label": null,
      "suggested_replacement": null,
      "expansion": null
    }
  ],
  "children": [
    {
      "message": "#[warn(dead_code)] on by default",
      "code": null,
      "level": "note",
      "spans": [],
      "children": [],
      "rendered": null
    }
  ],
  "rendered": "warning: constant item is never used: `FOOBAR`\n  --> src/main.rs:1:1\n   |\n1  | / const FOOBAR: &'static str = r\"\n2  | | A\n3  | | B\n4  | | C\n...  |\n11 | | J\n12 | | \";\n   | |__^\n   |\n   = note: #[warn(dead_code)] on by default\n\n"
}
"##;
#[cfg(test)]
static TEST_LONG_MULTILINE_EXPECTED: &'static str = r##"
warning: constant item is never used: `FOOBAR`
  --> src/main.rs:1:1
   |
1  | / const FOOBAR: &'static str = r"
2  | | A
3  | | B
4  | | C
...  |
11 | | J
12 | | ";
   | |__^
   |
   = note: #[warn(dead_code)] on by default
"##;
#[test]
fn test_long_multiline() {
    test_message_printing( TEST_LONG_MULTILINE_JSON, TEST_LONG_MULTILINE_EXPECTED );
}
