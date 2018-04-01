#[macro_use]
extern crate stdweb;

fn run( input: f64 ) -> f64 {
    let mut out = 3.14;

    out *= 10.2_f64 % (input as f64);
    out *= input.sin();
    out *= input.cos();
    out *= input.tan();
    out *= input.asin();
    out *= input.acos();
    out *= input.atan();
    out *= input.atan2( 2.0 );
    out *= input.exp();
    out *= input.exp2();
    out *= input.sqrt();
    out *= input.powf( 2.0 );
    out *= input.powf( 3.0 );
    out *= input.ln();
    out *= input.log2();
    out *= input.log10();
    out *= input.cbrt();
    out *= input.hypot( 2.0 );
    out *= input.sinh();
    out *= input.cosh();
    out *= input.tanh();
    out *= input.asinh();
    out *= input.acosh();
    out *= input.atanh();
    out *= input.round();

    out *= (10.2_f32 % (input as f32)) as f64;
    out *= (input as f32).sin() as f64;
    out *= (input as f32).cos() as f64;
    out *= (input as f32).tan() as f64;
    out *= (input as f32).asin() as f64;
    out *= (input as f32).acos() as f64;
    out *= (input as f32).atan() as f64;
    out *= (input as f32).atan2( 2.0 ) as f64;
    out *= (input as f32).exp() as f64;
    out *= (input as f32).exp2() as f64;
    out *= (input as f32).sqrt() as f64;
    out *= (input as f32).powf( 2.0 ) as f64;
    out *= (input as f32).powf( 3.0 ) as f64;
    out *= (input as f32).ln() as f64;
    out *= (input as f32).log2() as f64;
    out *= (input as f32).log10() as f64;
    out *= (input as f32).cbrt() as f64;
    out *= (input as f32).hypot( 2.0 ) as f64;
    out *= (input as f32).sinh() as f64;
    out *= (input as f32).cosh() as f64;
    out *= (input as f32).tanh() as f64;
    out *= (input as f32).asinh() as f64;
    out *= (input as f32).acosh() as f64;
    out *= (input as f32).atanh() as f64;
    out *= (input as f32).round() as f64;

    out
}

fn main() {
    stdweb::initialize();

    js! {
        Module.exports.run = @{run};
    }
}
