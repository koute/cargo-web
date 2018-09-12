#[macro_use]
extern crate stdweb;

#[cfg(test)]
#[no_mangle]
#[allow(dead_code)]
#[allow(non_snake_case)]
pub fn __async_test__ok() {
    js! {
        ASYNC_TEST_PRIVATE.resolve();
    }
}

#[cfg(test)]
#[no_mangle]
#[allow(dead_code)]
#[allow(non_snake_case)]
pub fn __async_test__reject() {
    js! {
        ASYNC_TEST_PRIVATE.reject( "Test explicitly rejected" );
    }
}

#[cfg(test)]
#[no_mangle]
#[allow(dead_code)]
#[allow(non_snake_case)]
pub fn __async_test__panic() {
    stdweb::initialize();
    assert!( false );
}

#[cfg(test)]
#[no_mangle]
#[allow(dead_code)]
#[allow(non_snake_case)]
pub fn __async_test__timeout() {
}

#[test]
pub fn normal_test() {
}
