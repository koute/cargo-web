use std::fmt::Write;
use serde_json::{self, Value};
use semver;
use serde::{ser, de, Serializer};

pub use cargo_shim::rustc_diagnostic::Diagnostic;

// TODO: Upstream `Serializable` for these to `cargo-metadata`.

#[derive(Clone, Serialize, Deserialize, Debug)]
/// A single target (lib, bin, example, ...) provided by a crate
pub struct Target {
    /// Name as given in the `Cargo.toml` or generated from the file name
    pub name: String,
    /// Kind of target ("bin", "example", "test", "bench", "lib")
    pub kind: Vec<String>,
    /// Almost the same as `kind`, except when an example is a library instad of an executable.
    /// In that case `crate_types` contains things like `rlib` and `dylib` while `kind` is `example`
    #[serde(default)]
    pub crate_types: Vec<String>,
    /// Path to the main source file of the target
    pub src_path: String,
    #[doc(hidden)]
    #[serde(skip)]
    __do_not_match_exhaustively: (),
}

#[derive(Clone, Debug)]
/// A workspace member. This is basically identical to `cargo::core::package_id::PackageId`, expect
/// that this does not use `Arc` internally.
pub struct PackageId {
    /// A name of workspace member.
    pub name: String,
    /// A version of workspace member.
    pub version: semver::Version,
    /// A source id of workspace member.
    pub url: String,
    #[doc(hidden)]
    __do_not_match_exhaustively: (),
}

impl<'de> de::Deserialize<'de> for PackageId {
    fn deserialize<D>(d: D) -> Result<PackageId, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        let string = String::deserialize(d)?;
        let mut s = string.splitn(3, ' ');
        let name = s.next().unwrap();
        let version = s.next().unwrap();
        let version = semver::Version::parse(&version).map_err(de::Error::custom)?;
        let url = &s.next().unwrap();
        let url = &url[1..url.len() - 1];
        Ok(PackageId {
            name: name.to_owned(),
            version: version,
            url: url.to_owned(),
            __do_not_match_exhaustively: (),
        })
    }
}

impl ser::Serialize for PackageId {
    fn serialize< S >( &self, serializer: S ) -> Result< S::Ok, S::Error >
    where
        S: Serializer
    {
        let mut output = String::new();
        write!( output, "{} {} ({})", self.name, self.version, self.url ).unwrap();
        serializer.serialize_str( &output )
    }
}

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
