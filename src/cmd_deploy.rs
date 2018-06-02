use std::fs;

use clap;

use cargo_shim::{
    Profile,
    TargetKind
};

use build::{
    BuildArgs,
    Backend,
};
use deployment::Deployment;
use error::Error;

pub fn command_deploy< 'a >( matches: &clap::ArgMatches< 'a > ) -> Result< (), Error > {
    let build_args = BuildArgs::new( matches )?;
    let project = build_args.load_project()?;

    let package = project.package();
    let targets = project.target_or_select( |target| {
        target.kind == TargetKind::Bin ||
        (target.kind == TargetKind::CDyLib && project.backend().is_native_wasm())
    })?;

    let config = project.aggregate_configuration( Profile::Main )?;
    let target = targets[ 0 ];
    let result = project.build( &config, target )?;

    let js_wasm_path = project.js_wasm_path();
    let serve_url = project.serve_url();
    let is_emscripten_wasm = project.backend() == Backend::EmscriptenWebAssembly;
    let deployment = Deployment::new( package, target, &result, &js_wasm_path, &serve_url, is_emscripten_wasm )?;

    let (default, directory) = project.deploy_path()?;
    if default && directory.exists() {
        let entries = fs::read_dir( &directory ).map_err( |error| Error::CannotRemoveDirectory( directory.clone(), error ) )?; // TODO: Another error?
        for entry in entries {
            let entry = entry.expect( "cannot unwrap directory entry" );
            let path = entry.path();
            if path.is_dir() {
                fs::remove_dir_all( &path ).map_err( |error| Error::CannotRemoveDirectory( path.clone(), error ) )?;
            } else {
                fs::remove_file( &path ).map_err( |error| Error::CannotRemoveFile( path.clone(), error ) )?;
            }
        }
    }

    deployment.deploy_to( &directory )?;

    eprintln!( "The `{}` was deployed to {:?}!", target.name, directory );
    Ok(())
}
