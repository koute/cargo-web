use reqwest;
use serde::Serialize;
use serde_json::{self, Value};
use serde_json::error::Error as JsonError;

use std::thread;
use std::sync::mpsc::{self, channel};
use std::fmt;
use std::error::Error;
use std::time::Duration;

use websocket::{Message, OwnedMessage};
use websocket::client::ClientBuilder;
use websocket::result::WebSocketError;

#[derive(Debug)]
pub enum ConnectionError {
    FailedToFetchUrl( reqwest::Error ),
    WebSocketError( WebSocketError )
}

impl Error for ConnectionError {
    fn description( &self ) -> &str {
        match *self {
            ConnectionError::FailedToFetchUrl( _ ) => "failed to fetch websocket debugger URL",
            ConnectionError::WebSocketError( _ ) => "web socket error"
        }
    }
}

impl fmt::Display for ConnectionError {
    fn fmt( &self, fmt: &mut fmt::Formatter ) -> fmt::Result {
        match *self {
            ConnectionError::FailedToFetchUrl( ref message ) => write!( fmt, "{}: {}", self.description(), message ),
            ConnectionError::WebSocketError( ref message ) => write!( fmt, "{}: {}", self.description(), message )
        }
    }
}

impl From< reqwest::Error > for ConnectionError {
    fn from( error: reqwest::Error ) -> Self {
        ConnectionError::FailedToFetchUrl( error )
    }
}

impl From< WebSocketError > for ConnectionError {
    fn from( error: WebSocketError ) -> Self {
        ConnectionError::WebSocketError( error )
    }
}

#[derive(Debug)]
pub enum CommunicationError {
    Send( WebSocketError ),
    Recv( WebSocketError )
}

impl Error for CommunicationError {
    fn description( &self ) -> &str {
        match *self {
            CommunicationError::Send( _ ) => "error while sending web socket message",
            CommunicationError::Recv( _ ) => "error while receiving web socket message"
        }
    }
}

impl fmt::Display for CommunicationError {
    fn fmt( &self, fmt: &mut fmt::Formatter ) -> fmt::Result {
        match *self {
            CommunicationError::Send( ref message ) => write!( fmt, "{}: {}", self.description(), message ),
            CommunicationError::Recv( ref message ) => write!( fmt, "{}: {}", self.description(), message )
        }
    }
}

#[derive(Debug)]
pub enum ReplyError {
    Timeout,
    MalformedMessage( &'static str ),
    MalformedJson( JsonError ),
    CommunicationError( CommunicationError )
}

impl Error for ReplyError {
    fn description( &self ) -> &str {
        match *self {
            ReplyError::Timeout => "timeout while waiting for reply",
            ReplyError::MalformedMessage( _ ) => "received malformed message",
            ReplyError::MalformedJson( _ ) => "received malformed JSON",
            ReplyError::CommunicationError( _ ) => "communication error"
        }
    }
}

impl fmt::Display for ReplyError {
    fn fmt( &self, fmt: &mut fmt::Formatter ) -> fmt::Result {
        match *self {
            ReplyError::Timeout => write!( fmt, "{}", self.description() ),
            ReplyError::MalformedMessage( ref error ) => write!( fmt, "{}: {}", self.description(), error ),
            ReplyError::MalformedJson( ref error ) => write!( fmt, "{}: {}", self.description(), error ),
            ReplyError::CommunicationError( ref error ) => write!( fmt, "{}: {}", self.description(), error )
        }
    }
}

impl From< CommunicationError > for ReplyError {
    fn from( error: CommunicationError ) -> Self {
        ReplyError::CommunicationError( error )
    }
}

impl From< JsonError > for ReplyError {
    fn from( error: JsonError ) -> Self {
        ReplyError::MalformedJson( error )
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub struct RequestId( u64 );

#[derive(Clone, Debug)]
pub enum Reply {
    Event {
        method: String,
        body: Value
    },
    Result {
        id: RequestId,
        body: Value
    },
    Error {
        id: RequestId,
        code: i64,
        message: String
    }
}

pub struct Connection {
    tx: mpsc::Sender< OwnedMessage >,
    rx: mpsc::Receiver< Result< OwnedMessage, CommunicationError > >,
    last_id: u64
}

fn owned_message_to_json( message: Result< OwnedMessage, CommunicationError > ) -> Result< Value, ReplyError > {
    match message? {
        OwnedMessage::Text( text ) => {
            match serde_json::from_str( &text ) {
                Ok( value ) => return Ok( value ),
                Err( error ) => return Err( error.into() )
            }
        },
        _ => return Err( ReplyError::MalformedMessage( "non text message received" ) )
    }
}

impl Connection {
    pub fn connect( json_url: &str ) -> Result< Self , ConnectionError > {
        let mut response = reqwest::get( json_url )?;
        let text = response.text()?;
        let json: Value = serde_json::from_str( &text ).unwrap();
        let url = json.get( 0 ).unwrap().get( "webSocketDebuggerUrl" ).unwrap().as_str().unwrap();
        debug!( "Got websocket debugger URL {}", url );

        let client = ClientBuilder::new( &url ).unwrap().connect_insecure()?;
        debug!( "Connected to: {}", url );

        let (mut receiver, mut sender) = client.split().unwrap();

        let (output_tx, output_rx) = channel();
        let (input_tx, input_rx) = channel();

        let input_tx_clone = input_tx.clone();
        thread::spawn( move || {
            loop {
                let message = match output_rx.recv() {
                    Ok( message ) => message,
                    Err( _ ) => break
                };

                debug!( "Sending: {:?}", message );
                if let Err( error ) = sender.send_message( &message ) {
                    let _ = input_tx_clone.send( Err( CommunicationError::Send( error ) ) );
                    let _ = sender.send_message( &Message::close() );
                    break;
                }

                if let OwnedMessage::Close( _ ) = message {
                    break;
                }
            }
        });

        let output_tx_clone = output_tx.clone();
        thread::spawn( move || {
            for message in receiver.incoming_messages() {
                debug!( "Received: {:?}", message );
                let message = match message {
                    Ok( message ) => message,
                    Err( error ) => {
                        let _ = input_tx.send( Err( CommunicationError::Recv( error ) ) );
                        let _ = output_tx_clone.send( OwnedMessage::Close( None ) );
                        break;
                    }
                };

                match message {
                    message @ OwnedMessage::Close( _ ) => {
                        let _ = output_tx_clone.send( message.clone() );
                        let _ = input_tx.send( Ok( message ) );
                        break;
                    }
                    message @ OwnedMessage::Ping( _ ) => {
                        match output_tx_clone.send( message ) {
                            Ok(()) => {},
                            Err( _ ) => break
                        }
                    }
                    message => {
                        match input_tx.send( Ok( message ) ) {
                            Ok(()) => {},
                            Err( _ ) => break
                        }
                    },
                }
            }
        });

        Ok( Connection {
            tx: output_tx,
            rx: input_rx,
            last_id: 0
        })
    }

    fn raw_send( &mut self, message: OwnedMessage ) {
        let _ = self.tx.send( message );
    }

    pub fn send_cmd< T: Serialize >( &mut self, method: &str, params: T ) -> RequestId {
        #[derive(Serialize)]
        struct Command< T: Serialize > {
            method: String,
            params: T,
            id: u64
        }

        let id = self.last_id;
        let command = Command {
            method: method.to_owned(),
            params,
            id
        };

        self.last_id += 1;
        let message = serde_json::to_string( &command ).unwrap();
        self.raw_send( OwnedMessage::Text( message ) );

        RequestId( id )
    }

    fn raw_try_recv( &mut self, wait_for: Option< Duration > ) -> Option< Result< OwnedMessage, CommunicationError > > {
        if let Some( wait_for ) = wait_for {
            match self.rx.recv_timeout( wait_for ) {
                Ok( message ) => Some( message ),
                Err( _ ) => None
            }
        } else {
            match self.rx.try_recv() {
                Ok( message ) => Some( message ),
                Err( _ ) => None
            }
        }
    }

    fn json_try_recv( &mut self, wait_for: Option< Duration > ) -> Result< Value, ReplyError > {
        let message = self.raw_try_recv( wait_for );
        match message {
            Some( message ) => return owned_message_to_json( message ),
            None => return Err( ReplyError::Timeout )
        }
    }

    pub fn try_recv( &mut self, wait_for: Option< Duration > ) -> Result< Reply, ReplyError > {
        let json = self.json_try_recv( wait_for )?;
        if let Some( error ) = json.get( "error" ) {
            let id = json.get( "id" )
                .ok_or( ReplyError::MalformedMessage( "'id' field not found" ) )?
                .as_u64()
                .ok_or( ReplyError::MalformedMessage( "'id' field is not an integer" ) )?;
            let id = RequestId( id );
            let code = error.get( "code" )
                .ok_or( ReplyError::MalformedMessage( "'error.code' field not found" ) )?
                .as_i64()
                .ok_or( ReplyError::MalformedMessage( "'error.code' field is not an integer" ) )?;
            let message = error.get( "message" )
                .ok_or( ReplyError::MalformedMessage( "'error.message' field not found" ) )?
                .as_str()
                .ok_or( ReplyError::MalformedMessage( "'error.message' field is not an integer" ) )?
                .to_owned();
            Ok( Reply::Error { id, code, message } )
        } else if let Some( id ) = json.get( "id" ) {
            let id = id.as_u64().ok_or( ReplyError::MalformedMessage( "'id' field is not an integer" ) )?;
            let id = RequestId( id );
            let body = json.get( "result" ).ok_or( ReplyError::MalformedMessage( "'body' field not found" ) )?.clone();
            Ok( Reply::Result { id, body } )
        } else {
            let method = json.get( "method" )
                .ok_or( ReplyError::MalformedMessage( "'method' field not found" ) )?
                .as_str()
                .ok_or( ReplyError::MalformedMessage( "'method' field is not a string" ) )?
                .to_owned();
            let body = json.get( "params" ).ok_or( ReplyError::MalformedMessage( "'params' field not found" ) )?.clone();
            Ok( Reply::Event { method, body } )
        }
    }
}

// https://chromedevtools.github.io/devtools-protocol/tot/Runtime/#type-RemoteObject
#[derive(Clone, Deserialize, Debug)]
pub struct RemoteObject {
    #[serde(rename = "type")]
    pub kind: String,
    pub value: Option< Value >,
    #[serde(rename = "className")]
    pub class_name: Option< String >,
    pub description: Option< String >
}

// https://chromedevtools.github.io/devtools-protocol/tot/Runtime/#event-consoleAPICalled
#[derive(Clone, Deserialize, Debug)]
pub struct ConsoleApiCalledBody {
    #[serde(rename = "type")]
    pub kind: String,
    pub args: Vec< RemoteObject >
}

// https://chromedevtools.github.io/devtools-protocol/tot/Runtime/#event-exceptionThrown
#[derive(Clone, Deserialize, Debug)]
pub struct ExceptionThrownBody {
    #[serde(rename = "exceptionDetails")]
    pub exception_details: ExceptionDetails
}

// https://chromedevtools.github.io/devtools-protocol/tot/Runtime/#type-ExceptionDetails
#[derive(Clone, Deserialize, Debug)]
pub struct ExceptionDetails {
    pub text: String,
    pub exception: Option< RemoteObject >,
    #[serde(rename = "lineNumber")]
    pub line_number: u32,
    #[serde(rename = "columnNumber")]
    pub column_number: u32,
    #[serde(rename = "scriptId")]
    pub script_id: Option< String >,
    pub url: Option< String >
}
