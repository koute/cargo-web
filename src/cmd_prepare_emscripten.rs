use emscripten::initialize_emscripten;
use error::Error;

pub fn command_prepare_emscripten< 'a >() -> Result< (), Error > {
    match initialize_emscripten( false, true ) {
        None => return Err( Error::EmscriptenNotAvailable ),
        Some( _emscripten ) => return Ok( () ),
    }
}
