use std::io;
use std::sync::Arc;
use std::fs::File;
use std::net::SocketAddr;
use std::time::UNIX_EPOCH;
use futures::{Poll, Async};
use futures::future::{self, Future};
use futures::stream::Stream;
use hyper::{self, StatusCode};
use hyper::header::{CacheControl, CacheDirective, ContentLength, ContentType, Expires, Pragma};
use hyper::server::{Http, NewService, Request, Service};
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

pub struct Body( Option< BodyContents > );

impl Stream for Body {
    type Item = BodyContents;
    type Error = hyper::error::Error;

    fn poll( &mut self ) -> Poll< Option< Self::Item >, Self::Error > {
        Ok( Async::Ready( self.0.take() ) )
    }
}

impl From< Vec< u8 > > for Body {
    fn from( buffer: Vec< u8 > ) -> Self {
        Body( Some( BodyContents::Owned( buffer ) ) )
    }
}

impl From< Mmap > for Body {
    fn from( map: Mmap ) -> Self {
        Body( Some( BodyContents::Mmap( map ) ) )
    }
}

type Response = hyper::server::Response< Body >;

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
    server: hyper::Server< NewSimpleService, Body >
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

pub fn response_from_file( mime_type: &str, fp: File ) -> FutureResponse {
    let map = unsafe { Mmap::map( &fp ) }.expect( "mmap failed" );
    let length = map.len();
    let body: Body = map.into();
    let response = add_no_cache_headers( Response::new() )
        .with_header( ContentType( mime_type.parse().unwrap() ) )
        .with_header( ContentLength( length as u64 ) )
        .with_body( body );

    Box::new( future::ok( response ) )
}

fn sync_response_from_data( mime_type: &str, data: Vec< u8 > ) -> Response {
    let length = data.len();
    let body: Body = data.into();
    add_no_cache_headers( Response::new() )
        .with_header( ContentType( mime_type.parse().unwrap() ) )
        .with_header( ContentLength( length as u64 ) )
        .with_body( body )
}

pub fn response_from_data( mime_type: &str, data: Vec< u8 > ) -> FutureResponse {
    Box::new( future::ok( sync_response_from_data( mime_type, data ) ) )
}

pub fn response_from_status( status: StatusCode ) -> FutureResponse {
    Box::new( future::ok(
        sync_response_from_data( "text/plain", format!( "{}", status ).into_bytes() ).with_status( status )
    ) )
}
