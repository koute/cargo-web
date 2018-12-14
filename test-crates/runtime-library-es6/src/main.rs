#[macro_use]
extern crate stdweb;

#[js_export]
pub fn add( a: i32, b: i32 ) -> i32 {
    a + b
}

fn main() {
    js! {
        console.log( "Main triggered!" );
    }
}
