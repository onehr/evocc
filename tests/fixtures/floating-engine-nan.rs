// Candidate-engine qualification, not the admitted finite-constant API.
use rustc_apfloat::{ieee::Double, Float, Round};

fn main() {
    for (name, a, b) in [
        ("q1-q2", 0x7ff8000000000011u128, 0xfff8000000000022u128),
        ("q1-s2", 0x7ff8000000000011, 0xfff0000000000022),
        ("s1-q2", 0x7ff0000000000011, 0xfff8000000000022),
    ] {
        let a = Double::from_bits(a);
        let b = Double::from_bits(b);
        let sum = a.add_r(b, Round::NearestTiesToEven);
        let product = a.mul_r(b, Round::NearestTiesToEven);
        let quotient = a.div_r(b, Round::NearestTiesToEven);
        println!("{name} add={:016x} mul={:016x} div={:016x}",
                 sum.value.to_bits(), product.value.to_bits(), quotient.value.to_bits());
    }
}
