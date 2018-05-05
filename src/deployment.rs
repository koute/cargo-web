use std::collections::BTreeMap;
use std::path::{PathBuf, Path};
use std::io::{self, Read, Write};
use std::fs::{self, File};

use handlebars::Handlebars;
use walkdir::WalkDir;

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
<html>
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
    handlebars.render_template( DEFAULT_INDEX_HTML_TEMPLATE, &template_data ).unwrap()
}

pub struct DeployWithServePath {
    js_key: Option<String>,
    wasm_file_name: Option<String>,
    serve_path: Option<String>
}

enum RouteKind {
    Blob( Vec< u8 > ),
    StaticDirectory( PathBuf )
}

struct Route {
    key: String,
    kind: RouteKind,
    can_be_deployed: bool
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

impl DeployWithServePath {
    pub fn new(serve_path: &Option<String>) -> Result<Self, Error> {
        use std::path::MAIN_SEPARATOR as SEP;

        let serve_path = if let Some(ref path) = *serve_path {
            let mut double_sep = SEP.to_string();
            double_sep.push(SEP);

            // As state in README.md
            // serve-path is sub and relative to deploy-path
            // It is not allow to contains `\\` or `..`
            if path.contains( &double_sep ) || path.contains( ".." ) {
                return Err( Error::ConfigurationError( format!("serve-path is invalid: {}", path) ) );
            }

            let mut path = if path.starts_with( &SEP.to_string() ) {
                path[1..].trim().to_string()
            } else {
                path.trim().to_string()
            };
            if path.len() > 0 {
                if !path.ends_with( &SEP.to_string() ) {
                    path.push( SEP );
                }
                Some(path)
            } else {
                None
            }
        } else {
            None
        };

        Ok(Self {
            js_key: None,
            wasm_file_name: None,
            serve_path
        })
    }

    fn create_serve_path_for(&mut self, name: &str, ext: &::std::ffi::OsStr) -> String {
        let serve_path = match self.serve_path {
            Some(ref path) => path,
            None => return name.to_string()
        };
        let with_path = PathBuf::from( serve_path ).join( name ).to_string_lossy().to_string();
        if ext == "js" {
            self.js_key = Some( with_path.clone() );
        } else if ext == "wasm" {
            self.wasm_file_name = Some( name.to_string() );
        }
        with_path
    }

    fn insert_serve_path_to_js(&self, routes: &mut Vec<Route>) {
        let serve_path = match self.serve_path {
            Some(ref path) => path,
            None => return
        };

        let js_key = if let Some(ref js_key) = self.js_key {
            js_key
        } else {
            return;
        };

        let wasm_u8 = if let Some(ref wasm_file_name) = self.wasm_file_name {
            wasm_file_name.as_bytes()
        } else {
            return;
        };

        let _buf = PathBuf::from( serve_path ); // because of temporary value does not live long enough
        let _lossy = _buf.to_string_lossy(); // because of temporary value does not live long enough
        let serve_path_u8 = _lossy.as_bytes();

        // Search backward for value in contents
        fn search_start_index(contents: &[u8], value: &[u8], start_at: usize) -> Option<usize> {
            for index in (0..start_at).rev() {
                if contents[index..].starts_with(value) {
                    return Some(index);
                }
            }
            None
        }

        let js_route = routes.iter_mut().find(|r| r.key == *js_key).unwrap();
        match js_route.kind {
            RouteKind::Blob(ref mut contents) => {
                contents.reserve( serve_path_u8.len() * 2 );

                if let Some(start_index) 
                    = search_start_index( &contents[..], wasm_u8, contents.len()-wasm_u8.len() )
                {
                    contents.splice( start_index..start_index, serve_path_u8.iter().cloned() ).collect::<Vec<u8>>();

                    if let Some(start_index)
                        = search_start_index( &contents[..], wasm_u8, start_index )
                    {
                        contents.splice( start_index..start_index, serve_path_u8.iter().cloned() ).collect::<Vec<u8>>();
                    }

                }
            }
            _ => unreachable!()
        }
    }
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
    pub fn new(
            package: &CargoPackage,
            target: &CargoTarget,
            result: &CargoResult,
            mut with_serve_path: Option<DeployWithServePath>
        ) -> Result< Self, Error >
    {
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
                Some( ext ) if ext == "js" => {
                    if let Some(ref mut with_serve_path) = with_serve_path {
                        (true, with_serve_path.create_serve_path_for( &js_name, ext ) )
                    } else {
                        (true, js_name.clone())
                    }
                },
                Some( ext ) if ext == "wasm" => {
                    if let Some(ref mut with_serve_path) = with_serve_path {
                        let wasm_name = path.file_name().unwrap().to_string_lossy();
                        (false, with_serve_path.create_serve_path_for( &wasm_name, ext ) )
                    } else {
                        (false, path.file_name().unwrap().to_string_lossy().into_owned())
                    }
                },
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
                    kind: RouteKind::Blob( contents.clone() ),
                    can_be_deployed: false
                });
            }

            routes.push( Route {
                key,
                kind: RouteKind::Blob( contents ),
                can_be_deployed: true
            });
        }

        if let Some(with_serve_path) = with_serve_path{
            with_serve_path.insert_serve_path_to_js(&mut routes);
        }

        if let Some( target_static_path ) = target_static_path {
            routes.push( Route {
                key: "".to_owned(),
                kind: RouteKind::StaticDirectory( target_static_path.to_owned() ),
                can_be_deployed: true
            });
        }

        routes.push( Route {
            key: "".to_owned(),
            kind: RouteKind::StaticDirectory( crate_static_path.to_owned() ),
            can_be_deployed: true
        });

        routes.push( Route {
            key: "index.html".to_owned(),
            kind: RouteKind::Blob( generate_index_html( &js_name ).into() ),
            can_be_deployed: true
        });

        Ok( Deployment {
            routes
        })
    }

    pub fn js_url( &self ) -> &str {
        let route = self.routes.iter().find( |route| route.can_be_deployed && route.key.ends_with( ".js" ) ).unwrap();
        &route.key
    }

    pub fn get_by_url( &self, mut url: &str ) -> Option< Artifact > {
        if url.starts_with( "/" ) {
            url = &url[ 1.. ];
        }

        if url == "" {
            url = "index.html";
        }

        // TODO: Support more mime types. Use the mime_guess crate.
        let mime_type =
            if url.ends_with( ".js" ) { "application/javascript" }
            else if url.ends_with( ".json" ) { "application/json" }
            else if url.ends_with( ".wasm" ) { "application/wasm" }
            else if url.ends_with( ".html" ) { "text/html" }
            else if url.ends_with( ".css" ) { "text/css" }
            else if url.ends_with( ".svg" ) { "image/svg+xml" }
            else if url.ends_with( ".png" ) { "image/png" }
            else if url.ends_with( ".gif" ) { "image/gif" }
            else if url.ends_with( ".jpeg" ) { "image/jpeg" }
            else if url.ends_with( ".jpg" ) { "image/jpeg" }
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

    pub fn deploy_to( &self, root_directory: &Path ) -> Result< (), Error > {
        for route in &self.routes {
            if !route.can_be_deployed {
                continue;
            }

            match route.kind {
                RouteKind::Blob( ref bytes ) => {
                    let mut target_path = root_directory.to_owned();
                    for chunk in route.key.split( "/" ) {
                        target_path = target_path.join( chunk );
                    }

                    if target_path.exists() {
                        continue;
                    }

                    let target_dir = target_path.parent().unwrap();
                    fs::create_dir_all( target_dir )
                        .map_err( |err| Error::CannotCreateFile( target_dir.to_owned(), err ) )?; // TODO: Different error type?

                    let mut fp = File::create( &target_path ).map_err( |err| Error::CannotCreateFile( target_path.to_owned(), err ) )?;
                    fp.write_all( &bytes ).map_err( |err| Error::CannotWriteToFile( target_path.to_owned(), err ) )?;
                },
                RouteKind::StaticDirectory( ref source_dir ) => {
                    if !source_dir.exists() {
                        continue;
                    }

                    for entry in WalkDir::new( source_dir ) {
                        let entry = entry.map_err( |err| {
                            let err_path = err.path().map( |path| path.to_owned() ).unwrap_or_else( || source_dir.clone() );
                            let err: io::Error = err.into();
                            Error::CannotLoadFile( err_path, err ) // TODO: Different error type?
                        })?;

                        let source_path = entry.path();
                        let relative_path = source_path.strip_prefix( source_dir ).unwrap();
                        let target_path = root_directory.join( relative_path );
                        if target_path.exists() {
                            continue;
                        }

                        if source_path.is_dir() {
                            fs::create_dir_all( &target_path )
                                .map_err( |err| Error::CannotCreateFile( target_path.to_owned(), err ) )?; // TODO: Different error type?

                            continue;
                        }

                        let target_dir = target_path.parent().unwrap();
                        fs::create_dir_all( target_dir )
                            .map_err( |err| Error::CannotCreateFile( target_dir.to_owned(), err ) )?; // TODO: Different error type?

                        fs::copy( &source_path, &target_path )
                            .map_err( |err| Error::CannotCopyFile( source_path.to_owned(), target_path.to_owned(), err ) )?;
                    }
                }
            }
        }

        Ok(())
    }
}
