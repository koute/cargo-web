use serde_json;

pub use cargo_shim::rustc_diagnostic::Diagnostic;
pub use cargo_metadata::WorkspaceMember as PackageId;
pub use cargo_metadata::Target;

#[derive(Deserialize, Debug)]
pub struct Profile {
    pub opt_level: String,
    pub debuginfo: Option< u32 >,
    pub debug_assertions: bool,
    pub overflow_checks: bool,
    pub test: bool
}

#[derive(Deserialize, Debug)]
pub struct Message {
    pub message: Diagnostic,
    pub package_id: PackageId,
    pub target: Target
}

#[derive(Deserialize, Debug)]
pub struct Artifact {
    pub package_id: PackageId,
    pub target: Target,
    pub profile: Profile,
    pub features: Vec< String >,
    pub filenames: Vec< String >,
    pub fresh: bool
}

#[derive(Deserialize, Debug)]
pub struct BuildScriptExecuted {
    pub package_id: PackageId,
    pub linked_libs: Vec< String >,
    pub linked_paths: Vec< String >,
    pub cfgs: Vec< String >,
    pub env: Vec< (String, String) >
}

#[derive(Debug)]
pub enum CargoOutput {
    Message( Message ),
    Artifact( Artifact ),
    BuildScriptExecuted( BuildScriptExecuted )
}

impl CargoOutput {
    pub fn parse( string: &str ) -> Option< CargoOutput > {
        let json: serde_json::Value = serde_json::from_str( &string ).expect( "failed to parse cargo output as JSON" );
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
