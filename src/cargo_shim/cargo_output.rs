use serde_json::{self, Value};
use serde::ser;

pub use super::rustc_diagnostic::Diagnostic;
pub use cargo_metadata::Target;
pub use cargo_metadata::WorkspaceMember as PackageId;

#[derive(Serialize, Deserialize, Debug)]
pub struct Profile {
    pub opt_level: String,
    pub debuginfo: Option< u32 >,
    pub debug_assertions: bool,
    pub overflow_checks: bool,
    pub test: bool
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Message {
    pub message: Diagnostic,
    pub package_id: PackageId,
    pub target: Target
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Artifact {
    pub package_id: PackageId,
    pub target: Target,
    pub profile: Profile,
    pub features: Vec< String >,
    pub filenames: Vec< String >,
    pub fresh: bool
}

#[derive(Serialize, Deserialize, Debug)]
pub struct BuildScriptExecuted {
    pub package_id: PackageId,
    pub linked_libs: Vec< String >,
    pub linked_paths: Vec< String >,
    pub cfgs: Vec< String >,
    pub env: Vec< (String, String) >
}

fn any_to_json< T: ser::Serialize >( obj: T, reason: &str ) -> Value {
    let mut value = serde_json::to_value( obj ).unwrap();
    value.as_object_mut().unwrap().insert( "reason".to_owned(), reason.into() );
    value
}

impl Message {
    pub fn to_json_value( &self ) -> Value { any_to_json( self, "message" ) }
}

impl Artifact {
    pub fn to_json_value( &self ) -> Value { any_to_json( self, "compiler-artifact" ) }
}

impl BuildScriptExecuted {
    pub fn to_json_value( &self ) -> Value { any_to_json( self, "build-script-executed" ) }
}

#[derive(Debug)]
pub enum CargoOutput {
    Message( Message ),
    Artifact( Artifact ),
    BuildScriptExecuted( BuildScriptExecuted )
}

impl CargoOutput {
    pub fn parse( string: &str ) -> Option< CargoOutput > {
        let json: Value = serde_json::from_str( &string ).expect( "failed to parse cargo output as JSON" );
        let reason = json.get( "reason" ).expect( "missing `reason` field in cargo output" ).as_str().expect( "`reason` field is not a string" );
        let output = match reason {
            "compiler-message" => {
                CargoOutput::Message( serde_json::from_str( &string ).expect( "failed to parse compiler message" ) )
            },
            "compiler-artifact" => {
                CargoOutput::Artifact( serde_json::from_str( &string ).expect( "failed to parse compiler artifact" ) )
            },
            "build-script-executed" => {
                CargoOutput::BuildScriptExecuted( serde_json::from_str( &string ).expect( "failed to parse build script result" ) )
            },
            _ => return None
        };

        Some( output )
    }
}
