use std::io::Cursor;
use std::sync::Arc;
use std::fs::File;
use std::net::SocketAddr;
use futures::{Poll, Async};
use futures::future::{self, Future};
use hyper::body::Payload;
use hyper::{self, StatusCode, Request, Response, Server};
use hyper::service::{NewService, Service};
use hyper::server::conn::AddrIncoming;
use hyper::header::{CONTENT_TYPE, CONTENT_LENGTH, CACHE_CONTROL, EXPIRES, PRAGMA, ACCESS_CONTROL_ALLOW_ORIGIN};
use http::response::Builder;
use memmap::Mmap;

pub enum BodyContents {
    Owned( Vec< u8 > ),
    Mmap( Mmap )
}

impl AsRef< [u8] > for BodyContents {
    fn as_ref( &self ) -> &[u8] {
        match *self {
            BodyContents::Owned( ref buffer ) => &buffer,
            BodyContents::Mmap( ref map ) => &map
        }
    }
}

pub struct Body( Option< Cursor< BodyContents > > );

impl Payload for Body {
    type Data = Cursor< BodyContents >;
    type Error = hyper::Error;

    fn poll_data(&mut self) -> Poll< Option< Self::Data >, Self::Error > {
        Ok( Async::Ready( self.0.take() ) )
    }
}

impl From< Vec< u8 > > for Body {
    fn from( buffer: Vec< u8 > ) -> Self {
        Body( Some( Cursor::new( BodyContents::Owned( buffer ) ) ) )
    }
}

impl From< Mmap > for Body {
    fn from( map: Mmap ) -> Self {
        Body( Some( Cursor::new( BodyContents::Mmap( map ) ) ) )
    }
}

pub type ResponseFuture = Box< Future< Item = Response< Body >, Error = hyper::Error > + Send >;
pub type FnHandler = Box< Fn( Request< hyper::Body > ) -> ResponseFuture + Send + Sync >;
pub type ServiceFuture = Box< Future< Item = SimpleService, Error = hyper::Error > + Send >;

pub struct SimpleService {
    handler: Arc< FnHandler >
}

impl Service for SimpleService {
    type ReqBody = hyper::Body;
    type ResBody = Body;
    type Error = hyper::Error;

    type Future = ResponseFuture;

    fn call( &mut self, request: Request< hyper::Body > ) -> ResponseFuture {
        ( *self.handler )( request )
    }
}

pub struct NewSimpleService {
    handler: Arc< FnHandler >
}

impl NewService for NewSimpleService {
    type ReqBody = hyper::Body;
    type ResBody = Body;
    type Error = hyper::Error;

    type Service = SimpleService;
    type Future = ServiceFuture;
    type InitError = hyper::Error;

    fn new_service( &self ) -> Self::Future {
        Box::new( future::ok( SimpleService {
            handler: self.handler.clone()
        } ) )
    }
}

pub struct SimpleServer {
    server: Server< AddrIncoming, NewSimpleService >
}

impl SimpleServer {
    pub fn new< F >( address: &SocketAddr, handler: F ) -> Self
    where
        F: Send + Sync + 'static + Fn( Request< hyper::Body > ) -> ResponseFuture
    {
        let server = Server::bind( address )
            .serve( NewSimpleService {
                handler: Arc::new( Box::new( handler ) )
            } );
        SimpleServer { server }
    }

    pub fn server_addr( &self ) -> SocketAddr {
        self.server.local_addr()
    }

    pub fn run( self ) {
        hyper::rt::run(self.server.map_err(|e| {
            eprintln!("server error: {}", e);
        }));
    }
}

fn add_headers( builder: &mut Builder ) {
    builder.header( CACHE_CONTROL, "no-cache" );
    builder.header( CACHE_CONTROL, "no-store" );
    builder.header( CACHE_CONTROL, "must-revalidate" );
    builder.header( EXPIRES, "0" );
    builder.header( PRAGMA, "no-cache" );
    builder.header( ACCESS_CONTROL_ALLOW_ORIGIN, "*" );
}

pub fn response_from_file( mime_type: &str, fp: File ) -> ResponseFuture {
    if let Ok( metadata ) = fp.metadata() {
        if metadata.len() == 0 {
            // This is necessary since `Mmap::map` will return an error for empty files.
            return response_from_data( mime_type, Vec::new() );
        }
    }

    let map = match unsafe { Mmap::map( &fp ) } {
        Ok( map ) => map,
        Err( error ) => {
            warn!( "Mmap failed: {}", error );
            let status = StatusCode::INTERNAL_SERVER_ERROR;
            let message = format!( "{}\n\n{}", status, error ).into_bytes();
            let mut response = sync_response_from_data( "text/plain", message );
            *response.status_mut() = status;
            return Box::new( future::ok( response ) );
        }
    };

    let length = map.len();
    let body: Body = map.into();
    let mut response = Response::builder();
    add_headers( &mut response );
    response.header( CONTENT_TYPE, mime_type );
    response.header( CONTENT_LENGTH, length );

    Box::new( future::ok( response.body( body ).unwrap() ) )
}

fn sync_response_from_data( mime_type: &str, data: Vec< u8 > ) -> Response< Body > {
    let length = data.len();
    let body: Body = data.into();
    let mut response = Response::builder();
    add_headers( &mut response );
    response.header( CONTENT_TYPE, mime_type );
    response.header( CONTENT_LENGTH, length );
    response.body( body ).unwrap()
}

pub fn response_from_data( mime_type: &str, data: Vec< u8 > ) -> ResponseFuture {
    Box::new( future::ok( sync_response_from_data( mime_type, data ) ) )
}

pub fn response_from_status( status: StatusCode ) -> ResponseFuture {
    let mut response = sync_response_from_data( "text/plain", format!( "{}", status ).into_bytes() );
    *response.status_mut() = status;
    Box::new( future::ok( response ) )
}
