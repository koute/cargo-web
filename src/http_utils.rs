use std::io::{self, Read};
use std::sync::Arc;
use std::fs::File;
use std::thread;
use std::net::SocketAddr;
use std::time::UNIX_EPOCH;
use futures::sync::{mpsc, oneshot};
use futures::Sink;
use futures::future::{self, Future};
use hyper::{self, Chunk, StatusCode};
use hyper::header::{CacheControl, CacheDirective, ContentLength, ContentType, Expires, Pragma};
use hyper::server::{Http, NewService, Request, Response, Service};

pub type FutureResponse = Box< Future< Item = Response, Error = hyper::Error > >;
pub type FnHandler = Box< Fn( Request ) -> FutureResponse + Send + Sync >;

pub struct SimpleService {
    handler: Arc< FnHandler >
}

impl Service for SimpleService {
    type Request = Request;
    type Response = Response;
    type Error = hyper::Error;

    type Future = FutureResponse;

    fn call( &self, request: Request ) -> FutureResponse {
        ( *self.handler )( request )
    }
}

pub struct NewSimpleService {
    handler: Arc< FnHandler >
}

impl NewService for NewSimpleService {
    type Request = Request;
    type Response = Response;
    type Error = hyper::Error;

    type Instance = SimpleService;

    fn new_service( &self ) -> Result< Self::Instance, io::Error > {
        Ok( SimpleService {
            handler: self.handler.clone()
        } )
    }
}

pub struct SimpleServer {
    server: hyper::Server< NewSimpleService, hyper::Body >
}

impl SimpleServer {
    pub fn new< F >( address: &SocketAddr, handler: F ) -> Self
    where
        F: Send + Sync + 'static + Fn( Request ) -> FutureResponse
    {
        let server = Http::new()
            .bind(
                address,
                NewSimpleService {
                    handler: Arc::new( Box::new( handler ) )
                }
            )
            .unwrap();
        SimpleServer { server }
    }

    pub fn server_addr( &self ) -> SocketAddr {
        self.server.local_addr().unwrap()
    }

    pub fn run( self ) {
        self.server.run().unwrap();
    }
}

fn add_no_cache_headers( response: Response ) -> Response {
    response
        .with_header( CacheControl( vec![
            CacheDirective::NoCache,
            CacheDirective::NoStore,
            CacheDirective::MustRevalidate,
        ] ) )
        .with_header( Expires( UNIX_EPOCH.into() ) )
        .with_header( Pragma::NoCache )
}

pub fn response_from_file( mime_type: &str, mut file: File ) -> FutureResponse {
    let ( tx, rx ) = oneshot::channel();
    let ( mut tx_body, rx_body ) = mpsc::channel( 1 );
    let response = add_no_cache_headers(Response::new())
        .with_header( ContentType( mime_type.parse().unwrap() ) )
        .with_body( rx_body );

    thread::spawn( move || {
        tx.send( response )
            .expect( "Send error on successful file read" );

        let mut buf = [ 0u8; 4096 ];
        while let Ok( n ) = file.read( &mut buf ) {
            if n == 0 {
                // eof
                tx_body.close().expect( "panic closing" );
                break;
            } else {
                let chunk: Chunk = buf.to_vec().into();
                match tx_body.send( Ok( chunk ) ).wait() {
                    Ok( t ) => {
                        tx_body = t;
                    }
                    Err( _ ) => {
                        break;
                    }
                };
            }
        }
    } );
    Box::new( rx.map_err( |e| hyper::Error::from( io::Error::new( io::ErrorKind::Other, e ) ) ) )
}

fn sync_response_from_data( mime_type: &str, data: Vec< u8 > ) -> Response {
    add_no_cache_headers(Response::new())
        .with_header( ContentType( mime_type.parse().unwrap() ) )
        .with_header( ContentLength( data.len() as u64 ) )
        .with_body( data )
}

pub fn response_from_data( mime_type: &str, data: Vec< u8 > ) -> FutureResponse {
    Box::new( future::ok( sync_response_from_data( mime_type, data ) ) )
}

pub fn response_from_status( status: StatusCode ) -> FutureResponse {
    Box::new( future::ok(
        sync_response_from_data( "text/plain", format!( "{}", status ).into_bytes() ).with_status( status )
    ) )
}
