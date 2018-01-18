use std::collections::BTreeMap;
use std::path::PathBuf;
use std::io::{self, Read};
use std::fs::File;

use handlebars::Handlebars;

use cargo_shim::{
    TargetKind,
    CargoPackage,
    CargoTarget,
    CargoResult
};

use error::Error;
use utils::read_bytes;

const DEFAULT_INDEX_HTML_TEMPLATE: &'static str = r#"
<!DOCTYPE html>
<head>
    <meta charset="utf-8" />
    <meta http-equiv="X-UA-Compatible" content="IE=edge" />
    <meta content="width=device-width, initial-scale=1.0, maximum-scale=1.0, user-scalable=1" name="viewport" />
    <script>
        var Module = {};
        var __cargo_web = {};
        Object.defineProperty( Module, 'canvas', {
            get: function() {
                if( __cargo_web.canvas ) {
                    return __cargo_web.canvas;
                }

                var canvas = document.createElement( 'canvas' );
                document.querySelector( 'body' ).appendChild( canvas );
                __cargo_web.canvas = canvas;

                return canvas;
            }
        });
    </script>
</head>
<body>
    <script src="{{{js_url}}}"></script>
</body>
</html>
"#;

fn generate_index_html( filename: &str ) -> String {
    let handlebars = Handlebars::new();
    let mut template_data = BTreeMap::new();
    template_data.insert( "js_url", filename.to_owned() );
    handlebars.template_render( DEFAULT_INDEX_HTML_TEMPLATE, &template_data ).unwrap()
}

enum RouteKind {
    Blob( Vec< u8 > ),
    StaticDirectory( PathBuf )
}

struct Route {
    key: String,
    kind: RouteKind
}

pub struct Deployment {
    routes: Vec< Route >
}

pub enum ArtifactKind {
    Data( Vec< u8 > ),
    File( File )
}

pub struct Artifact {
    pub mime_type: &'static str,
    pub kind: ArtifactKind
}

impl Artifact {
    pub fn map_text< F: FnOnce( String ) -> String >( self, callback: F ) -> io::Result< Self > {
        let mime_type = self.mime_type;
        let data = match self.kind {
            ArtifactKind::Data( data ) => data,
            ArtifactKind::File( mut fp ) => {
                let mut data = Vec::new();
                fp.read_to_end( &mut data )?;
                data
            }
        };

        let mut text = String::from_utf8_lossy( &data ).into_owned();
        text = callback( text );
        let data = text.into();
        Ok( Artifact {
            mime_type,
            kind: ArtifactKind::Data( data )
        })
    }
}

impl Deployment {
    pub fn new( package: &CargoPackage, target: &CargoTarget, result: &CargoResult ) -> Result< Self, Error > {
        let crate_static_path = package.crate_root.join( "static" );
        let target_static_path = match target.kind {
            TargetKind::Example => Some( target.source_directory.join( format!( "{}-static", target.name ) ) ),
            TargetKind::Bin => Some( target.source_directory.join( "static" ) ),
            _ => None
        };

        let js_name = format!( "{}.js", target.name );

        let mut routes = Vec::new();
        for path in result.artifacts() {
            let (is_js, key) = match path.extension() {
                Some( ext ) if ext == "js" => (true, js_name.clone()),
                Some( ext ) if ext == "wasm" => (false, path.file_name().unwrap().to_string_lossy().into_owned()),
                _ => continue
            };

            let contents = match read_bytes( &path ) {
                Ok( contents ) => contents,
                Err( error ) => return Err( Error::CannotLoadFile( path.clone(), error ) )
            };

            if is_js {
                // TODO: Remove this eventually. We're keeping it for now
                //       to not break compatibility with already written
                //       `index.html` files.
                routes.push( Route {
                    key: "js/app.js".to_owned(),
                    kind: RouteKind::Blob( contents.clone() )
                });
            }

            routes.push( Route {
                key,
                kind: RouteKind::Blob( contents )
            });
        }

        if let Some( target_static_path ) = target_static_path {
            routes.push( Route {
                key: "".to_owned(),
                kind: RouteKind::StaticDirectory( target_static_path.to_owned() )
            });
        }

        routes.push( Route {
            key: "".to_owned(),
            kind: RouteKind::StaticDirectory( crate_static_path.to_owned() )
        });

        routes.push( Route {
            key: "index.html".to_owned(),
            kind: RouteKind::Blob( generate_index_html( &js_name ).into() )
        });

        Ok( Deployment {
            routes
        })
    }

    pub fn get_by_url( &self, mut url: &str ) -> Option< Artifact > {
        if url.starts_with( "/" ) {
            url = &url[ 1.. ];
        }

        if url == "" {
            url = "index.html";
        }

        // TODO: Support more mime types. Steal the `extension_to_mime_impl` from `rouille`'s `assets.rs`.
        let mime_type =
            if url.ends_with( ".js" ) { "application/javascript" }
            else if url.ends_with( ".wasm" ) { "application/wasm" }
            else if url.ends_with( ".html" ) { "text/html" }
            else if url.ends_with( ".css" ) { "text/css" }
            else { "application/octet-stream" };

        for route in &self.routes {
            match route.kind {
                RouteKind::Blob( ref bytes ) => {
                    if url != route.key {
                        continue;
                    }

                    trace!( "Get by URL of {:?}: found blob", url );
                    return Some( Artifact {
                        mime_type,
                        kind: ArtifactKind::Data( bytes.clone() )
                    });
                },
                RouteKind::StaticDirectory( ref path ) => {
                    let mut target_path = path.clone();
                    for chunk in url.split( "/" ) {
                        target_path = target_path.join( chunk );
                    }

                    trace!( "Get by URL of {:?}: path {:?} exists: {}", url, target_path, target_path.exists() );
                    if target_path.exists() {
                        match File::open( &target_path ) {
                            Ok( fp ) => {
                                return Some( Artifact {
                                    mime_type,
                                    kind: ArtifactKind::File( fp )
                                });
                            },
                            Err( error ) => {
                                warn!( "Cannot open {:?}: {:?}", target_path, error );
                                return None;
                            }
                        }
                    }
                }
            }
        }

        trace!( "Get by URL of {:?}: not found", url );
        None
    }
}
